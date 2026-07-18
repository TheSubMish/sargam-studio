use crate::model::Stroke;

use super::modal_drum::{membrane_voicing, ModalDrumVoice, Voicing};
use super::{Instrument, Voice};

/// Madal — the double-headed barrel drum at the heart of most Nepali folk
/// ensembles.
///
/// Two heads: a larger low head ("din") and a smaller high head whose tuning
/// paste (khiran) loads the membrane into near-harmonic ratios — that's why
/// the madal's high head has a singing, definite pitch where the damphu is
/// just a thud. Bass = low head, Open = ringing high head, Rim = high head
/// edge, Slap/Mute = choked high head.
#[derive(Default)]
pub struct Madal;

const LOW_HEAD_HZ: f32 = 115.0;
const HIGH_HEAD_HZ: f32 = 235.0;

/// Unloaded membrane (low head): inharmonic.
const MEMBRANE_RATIOS: [f32; 5] = [1.0, 1.59, 2.14, 2.30, 2.65];
/// Khiran-loaded head: pulled toward harmonics, hence the tuned "ta".
const LOADED_RATIOS: [f32; 5] = [1.0, 2.0, 3.01, 4.05, 5.1];

fn voicing(stroke: Stroke, _velocity: f32) -> Voicing {
    match stroke {
        Stroke::Bass => membrane_voicing(LOW_HEAD_HZ, &MEMBRANE_RATIOS, 0.35, -0.65, 600.0),
        Stroke::Open => membrane_voicing(HIGH_HEAD_HZ, &LOADED_RATIOS, 0.50, -0.35, 1_800.0),
        Stroke::Rim => membrane_voicing(HIGH_HEAD_HZ * 1.9, &LOADED_RATIOS, 0.15, 0.3, 4_000.0),
        Stroke::Slap | Stroke::Mute => {
            membrane_voicing(HIGH_HEAD_HZ * 1.3, &MEMBRANE_RATIOS, 0.05, 0.25, 5_000.0)
        }
    }
}

impl Instrument for Madal {
    fn name(&self) -> &str {
        "Madal"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(ModalDrumVoice::new(sample_rate, 0x3ADA1, voicing))
    }
}
