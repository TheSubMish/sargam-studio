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
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(PlayerError::NoOutputDevice)?;
        let config = device
            .default_output_config()
            .map_err(|e| PlayerError::UnsupportedFormat(e.to_string()))?;
        if config.sample_format() != cpal::SampleFormat::F32 || config.channels() != 2 {
            return Err(PlayerError::UnsupportedFormat(format!(
                "{:?} {}ch (need f32 stereo)",
                config.sample_format(),
                config.channels()
            )));
        }
        let sample_rate = config.sample_rate().0 as f32;

        let (cmd_tx, cmd_rx) = HeapRb::<Command>::new(64).split();
        let (evt_tx, evt_rx) = HeapRb::<EngineEvent>::new(256).split();
        let mut core = EngineCore::new(cmd_rx, evt_tx);

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| core.process(data),
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| PlayerError::Stream(e.to_string()))?;
        stream.play().map_err(|e| PlayerError::Stream(e.to_string()))?;

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
