//! The instrument abstraction and the catalog of Nepali folk instruments.
//!
//! # Adding a new instrument
//!
//! 1. Create `src/instrument/<name>.rs` composing blocks from [`crate::dsp`]
//!    (add missing DSP algorithms to `dsp` first — they must stay reusable).
//! 2. Implement [`Instrument`] (the factory) and [`Voice`] (one sounding note).
//!    Melodic instruments answer `Trigger::Pitch`, percussion answers
//!    `Trigger::Stroke`; handle the other variant gracefully (map or ignore).
//!    Struck drums usually only need a `Voicing` table over the shared
//!    [`modal_drum::ModalDrumVoice`] — see madal/damphu.
//! 3. Declare the module below, re-export the type, and add it to [`catalog`].
//! 4. Add a rendering test/example proving it sounds right, and log it in CLAUDE.md.

pub mod bansuri;
pub mod damphu;
pub mod harmonium;
pub mod madal;
pub mod modal_drum;
pub mod ornament;
pub mod sampler;
pub mod tungna;

pub use bansuri::Bansuri;
pub use damphu::Damphu;
pub use harmonium::Harmonium;
pub use madal::Madal;
pub use sampler::{SamplerInstrument, SfzError};
pub use tungna::Tungna;

use std::sync::Arc;

use crate::model::NoteParams;

/// One currently-sounding note. Voices are pooled and reused; `tick` and
/// `note_on` run on the audio path, so implementations must not allocate or
/// lock inside them (pre-allocate in the `Instrument::make_voice` factory).
pub trait Voice: Send {
    /// Start (or restart — voices are recycled) sounding.
    fn note_on(&mut self, note: NoteParams);
    /// The player released/damped the note (struck/plucked instruments may
    /// simply keep decaying naturally).
    fn note_off(&mut self);
    /// Hard-stop to silence immediately: transport stop/seek, voice stealing.
    fn kill(&mut self);
    /// Produce the next mono sample.
    fn tick(&mut self) -> f32;
    /// False once the voice has decayed to silence and can be recycled.
    fn is_active(&self) -> bool;
}

/// An instrument: metadata plus a factory for its voices.
pub trait Instrument: Send + Sync {
    fn name(&self) -> &str;
    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice>;
}

/// Every instrument the composer currently offers, for UI listings and lookup.
pub fn catalog() -> Vec<Box<dyn Instrument>> {
    vec![
        Box::new(Tungna::default()),
        Box::new(Damphu),
        Box::new(Madal),
        Box::new(Harmonium::default()),
        Box::new(Bansuri),
    ]
}

/// Look up an instrument by (case-insensitive) catalog name — how tracks
/// reference their instrument in saved songs.
pub fn by_name(name: &str) -> Option<Box<dyn Instrument>> {
    catalog()
        .into_iter()
        .find(|i| i.name().eq_ignore_ascii_case(name))
}

/// Resolves track instrument names, extending the built-in catalog with
/// user-loaded instruments (SFZ sample packs). Registered names shadow
/// builtins, so a sampled "Madal" can replace the synthesized one.
#[derive(Default)]
pub struct Registry {
    extra: Vec<Arc<dyn Instrument>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, instrument: Arc<dyn Instrument>) {
        self.extra.push(instrument);
    }

    pub fn resolve(&self, name: &str) -> Option<Arc<dyn Instrument>> {
        self.extra
            .iter()
            .find(|i| i.name().eq_ignore_ascii_case(name))
            .cloned()
            .or_else(|| by_name(name).map(Arc::from))
    }
}
