//! File interchange: standard MIDI files (and later MusicXML, stems, ...).
//! Native project persistence lives on [`crate::model::Song`] itself (JSON).

pub mod midi;

pub use midi::{export_midi, import_midi, MidiError};
