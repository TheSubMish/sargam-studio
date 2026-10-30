//! cpal glue: owns the output stream and the control-side ends of the ring
//! buffers. All music logic lives in [`super::realtime::EngineCore`]; this file
//! only moves buffers.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use super::realtime::{Command, EngineCore, EngineEvent};

#[derive(Debug)]
pub enum PlayerError {
    NoOutputDevice,
    UnsupportedFormat(String),
    Stream(String),
}

impl std::fmt::Display for PlayerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => write!(f, "no audio output device found"),
            Self::UnsupportedFormat(s) => write!(f, "unsupported output format: {s}"),
            Self::Stream(s) => write!(f, "audio stream error: {s}"),
        }
    }
}

impl std::error::Error for PlayerError {}

pub struct Player {
    _stream: cpal::Stream,
    commands: HeapProd<Command>,
    events: HeapCons<EngineEvent>,
    pub sample_rate: f32,
}

impl Player {
    /// Open the default output device and start the engine (silent until a
    /// song is set and Play is sent).
    pub fn new() -> Result<Self, PlayerError> {
        Self::with_buffer_size(None)
    }

    /// Same as [`Self::new`], but tries to request a specific buffer size
    /// (in frames) for the output stream — lower means less latency, at the
    /// risk of underruns on a loaded system. Silently falls back to the
    /// device default if the requested size is unsupported or the device
    /// doesn't report a fixed range at all; this is a best-effort latency
    /// preference, not a hard requirement.
    pub fn with_buffer_size(preferred_buffer_frames: Option<u32>) -> Result<Self, PlayerError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| PlayerError::UnsupportedFormat(e.to_string()))?;
        if supported.sample_format() != cpal::SampleFormat::F32 || supported.channels() != 2 {
            return Err(PlayerError::UnsupportedFormat(format!(
                "{:?} {}ch (need f32 stereo)",
                supported.sample_format(),
                supported.channels()
            )));
        }
        let sample_rate = supported.sample_rate().0 as f32;

        let mut stream_config: cpal::StreamConfig = supported.config();
        if let Some(frames) = preferred_buffer_frames {
            if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
                stream_config.buffer_size = cpal::BufferSize::Fixed(frames.clamp(*min, *max));
            }
        }

        let (cmd_tx, cmd_rx) = HeapRb::<Command>::new(64).split();
        let (evt_tx, evt_rx) = HeapRb::<EngineEvent>::new(256).split();
        let mut core = EngineCore::new(cmd_rx, evt_tx);

        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _| core.process(data),
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| PlayerError::Stream(e.to_string()))?;
        stream
            .play()
            .map_err(|e| PlayerError::Stream(e.to_string()))?;

        Ok(Self {
            _stream: stream,
            commands: cmd_tx,
            events: evt_rx,
            sample_rate,
        })
    }

    /// Send a command; returns false if the queue was full (retry next frame).
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.try_push(command).is_ok()
    }

    pub fn poll_event(&mut self) -> Option<EngineEvent> {
        self.events.try_pop()
    }
}
