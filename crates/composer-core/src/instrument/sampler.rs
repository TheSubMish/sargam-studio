//! Sample-playback instrument driven by SFZ files.
//!
//! This is how real recordings of the instruments (Phase 5 sample packs) plug
//! in: an `.sfz` text file maps WAV files to key/velocity ranges with optional
//! sustain loops; [`SamplerInstrument::from_sfz`] loads it and behaves like any
//! other [`Instrument`]. Register loaded samplers in a
//! [`super::Registry`] so songs can reference them by name.
//!
//! Supported SFZ subset: `<global>/<group>/<region>` headers with opcodes
//! `sample`, `key`, `lokey`, `hikey`, `pitch_keycenter`, `lovel`, `hivel`,
//! `loop_mode` (`no_loop`/`loop_continuous`), `loop_start`, `loop_end`,
//! `ampeg_release`. Key opcodes are MIDI numbers (note names like `c4` and
//! sample paths containing spaces are not supported yet). Samples must be WAV.
//!
//! Percussion: stroke triggers map through [`Stroke::to_gm_key`], so a drum
//! SFZ laid out on GM keys (36/37/39/42/47) just works.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::{NoteParams, Trigger};

use super::{Instrument, Voice};

#[derive(Debug)]
pub enum SfzError {
    Io(std::io::Error),
    Wav(String),
    BadOpcode(String),
    NoRegions,
}

impl std::fmt::Display for SfzError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "sfz io error: {e}"),
            Self::Wav(s) => write!(f, "sfz sample error: {s}"),
            Self::BadOpcode(s) => write!(f, "sfz opcode error: {s}"),
            Self::NoRegions => write!(f, "sfz file defines no playable regions"),
        }
    }
}

impl std::error::Error for SfzError {}

#[derive(Clone, Copy, PartialEq)]
enum LoopMode {
    NoLoop,
    Continuous,
}

struct Region {
    data: Arc<Vec<f32>>,
    src_rate: f32,
    root_midi: f32,
    lokey: u8,
    hikey: u8,
    lovel: u8,
    hivel: u8,
    loop_mode: LoopMode,
    loop_start: f64,
    loop_end: f64,
    release_secs: f32,
}

pub struct SamplerInstrument {
    name: String,
    regions: Arc<Vec<Region>>,
}

impl SamplerInstrument {
    /// Load an SFZ file; the instrument's catalog name is the file stem
    /// (`madal_sampled.sfz` → "madal_sampled") unless overridden with
    /// [`Self::with_name`].
    pub fn from_sfz(path: &Path) -> Result<Self, SfzError> {
        let text = std::fs::read_to_string(path).map_err(SfzError::Io)?;
        let base_dir = path.parent().unwrap_or(Path::new("."));
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sampler".into());

        let mut sample_cache: HashMap<PathBuf, (Arc<Vec<f32>>, f32)> = HashMap::new();
        let mut regions = Vec::new();

        let mut global: HashMap<String, String> = HashMap::new();
        let mut group: HashMap<String, String> = HashMap::new();
        let mut region: Option<HashMap<String, String>> = None;

        // Strip // comments, then walk whitespace-separated tokens.
        let cleaned: String = text
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");

        #[derive(PartialEq)]
        enum Scope {
            Global,
            Group,
        }
        let mut scope = Scope::Global;

        let finalize =
            |region: &mut Option<HashMap<String, String>>,
             global: &HashMap<String, String>,
             group: &HashMap<String, String>,
             regions: &mut Vec<Region>,
             sample_cache: &mut HashMap<PathBuf, (Arc<Vec<f32>>, f32)>|
             -> Result<(), SfzError> {
                let Some(r) = region.take() else {
                    return Ok(());
                };
                let mut opts = global.clone();
                opts.extend(group.clone());
                opts.extend(r);
                regions.push(build_region(&opts, base_dir, sample_cache)?);
                Ok(())
            };

        for token in cleaned.split_whitespace() {
            match token {
                "<global>" => {
                    finalize(&mut region, &global, &group, &mut regions, &mut sample_cache)?;
                    scope = Scope::Global;
                }
                "<group>" => {
                    finalize(&mut region, &global, &group, &mut regions, &mut sample_cache)?;
                    group.clear();
                    scope = Scope::Group;
                }
                "<region>" => {
                    finalize(&mut region, &global, &group, &mut regions, &mut sample_cache)?;
                    region = Some(HashMap::new());
                }
                t => {
                    let Some((key, value)) = t.split_once('=') else {
                        return Err(SfzError::BadOpcode(format!("unexpected token: {t}")));
                    };
                    let target = match &mut region {
                        Some(r) => r,
                        None => match scope {
                            Scope::Global => &mut global,
                            Scope::Group => &mut group,
                        },
                    };
                    target.insert(key.to_ascii_lowercase(), value.to_string());
                }
            }
        }
        finalize(&mut region, &global, &group, &mut regions, &mut sample_cache)?;

        if regions.is_empty() {
            return Err(SfzError::NoRegions);
        }
        Ok(Self {
            name,
            regions: Arc::new(regions),
        })
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

fn opt_parse<T: std::str::FromStr>(
    opts: &HashMap<String, String>,
    key: &str,
    default: T,
) -> Result<T, SfzError> {
    match opts.get(key) {
        None => Ok(default),
        Some(v) => v
            .parse()
            .map_err(|_| SfzError::BadOpcode(format!("{key}={v}"))),
    }
}

fn build_region(
    opts: &HashMap<String, String>,
    base_dir: &Path,
    cache: &mut HashMap<PathBuf, (Arc<Vec<f32>>, f32)>,
) -> Result<Region, SfzError> {
    let sample = opts
        .get("sample")
        .ok_or_else(|| SfzError::BadOpcode("region missing sample=".into()))?;
    let sample_path = base_dir.join(sample.replace('\\', "/"));

    let (data, src_rate) = match cache.get(&sample_path) {
        Some(hit) => hit.clone(),
        None => {
            let loaded = load_wav_mono(&sample_path)?;
            cache.insert(sample_path.clone(), loaded.clone());
            loaded
        }
    };

    // key= sets the whole range + root at once; specific opcodes refine it.
    let key: Option<u8> = match opts.get("key") {
        Some(v) => Some(
            v.parse()
                .map_err(|_| SfzError::BadOpcode(format!("key={v}")))?,
        ),
        None => None,
    };
    let lokey = opt_parse(opts, "lokey", key.unwrap_or(0))?;
    let hikey = opt_parse(opts, "hikey", key.unwrap_or(127))?;
    let root_midi = opt_parse(opts, "pitch_keycenter", key.unwrap_or(60))? as f32;

    let loop_mode = match opts.get("loop_mode").map(String::as_str) {
        None | Some("no_loop") | Some("one_shot") => LoopMode::NoLoop,
        Some("loop_continuous") | Some("loop_sustain") => LoopMode::Continuous,
        Some(other) => return Err(SfzError::BadOpcode(format!("loop_mode={other}"))),
    };

    Ok(Region {
        loop_start: opt_parse(opts, "loop_start", 0u64)? as f64,
        loop_end: opt_parse(opts, "loop_end", data.len().saturating_sub(1) as u64)? as f64,
        release_secs: opt_parse(opts, "ampeg_release", 0.08f32)?,
        lovel: opt_parse(opts, "lovel", 0)?,
        hivel: opt_parse(opts, "hivel", 127)?,
        data,
        src_rate,
        root_midi,
        lokey,
        hikey,
        loop_mode,
    })
}

fn load_wav_mono(path: &Path) -> Result<(Arc<Vec<f32>>, f32), SfzError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|e| SfzError::Wav(format!("{}: {e}", path.display())))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| SfzError::Wav(e.to_string()))?,
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<Result<_, _>>()
                .map_err(|e| SfzError::Wav(e.to_string()))?
        }
    };

    // Fold to mono; per-track pan positions the sound in the mix instead.
    let mono: Vec<f32> = interleaved
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    Ok((Arc::new(mono), spec.sample_rate as f32))
}

impl Instrument for SamplerInstrument {
    fn name(&self) -> &str {
        &self.name
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(SamplerVoice {
            regions: Arc::clone(&self.regions),
            engine_rate: sample_rate,
            region: usize::MAX,
            pos: 0.0,
            rate: 1.0,
            gain: 0.0,
            release_level: 1.0,
            release_mult: 1.0,
            released: false,
            active: false,
        })
    }
}

struct SamplerVoice {
    regions: Arc<Vec<Region>>,
    engine_rate: f32,
    region: usize,
    pos: f64,
    rate: f64,
    gain: f32,
    release_level: f32,
    release_mult: f32,
    released: bool,
    active: bool,
}

impl Voice for SamplerVoice {
    fn note_on(&mut self, note: NoteParams) {
        let midi = match note.trigger {
            Trigger::Pitch(m) => m,
            Trigger::Stroke(s) => s.to_gm_key() as f32,
        };
        let key = midi.round().clamp(0.0, 127.0) as u8;
        let vel = (note.velocity * 127.0).round().clamp(0.0, 127.0) as u8;

        // Exact key+velocity match first, then any region covering the key.
        let matched = self
            .regions
            .iter()
            .position(|r| {
                (r.lokey..=r.hikey).contains(&key) && (r.lovel..=r.hivel).contains(&vel)
            })
            .or_else(|| {
                self.regions
                    .iter()
                    .position(|r| (r.lokey..=r.hikey).contains(&key))
            });
        let Some(idx) = matched else {
            self.active = false;
            return;
        };

        let r = &self.regions[idx];
        self.region = idx;
        self.pos = 0.0;
        self.rate =
            2.0_f64.powf(((midi - r.root_midi) / 12.0) as f64) * (r.src_rate / self.engine_rate) as f64;
        self.gain = note.velocity;
        self.release_level = 1.0;
        self.release_mult = 1.0;
        self.released = false;
        self.active = true;
    }

    fn note_off(&mut self) {
        if !self.active {
            return;
        }
        let r = &self.regions[self.region];
        self.released = true;
        // Reach -60 dB over the region's release time.
        let samples = (r.release_secs * self.engine_rate).max(1.0);
        self.release_mult = 0.001_f32.powf(1.0 / samples);
    }

    fn kill(&mut self) {
        self.active = false;
    }

    #[inline]
    fn tick(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let r = &self.regions[self.region];
        let data = &r.data;

        if self.pos as usize + 1 >= data.len() {
            self.active = false;
            return 0.0;
        }
        let i = self.pos as usize;
        let frac = (self.pos - i as f64) as f32;
        let sample = data[i] + frac * (data[i + 1] - data[i]);

        self.pos += self.rate;
        // Sustain loop: cycle until release, then play on through.
        if r.loop_mode == LoopMode::Continuous && !self.released && self.pos >= r.loop_end {
            self.pos -= r.loop_end - r.loop_start;
        }

        if self.released {
            self.release_level *= self.release_mult;
            if self.release_level < 1e-4 {
                self.active = false;
            }
        }
        sample * self.gain * self.release_level
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
