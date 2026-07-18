use crate::model::Stroke;

use super::modal_drum::{membrane_voicing, ModalDrumVoice, Voicing};
use super::{Instrument, Voice};

/// Damphu — the Tamang frame drum that drives selo rhythms.
///
/// A single goat-skin head on a shallow ring: inharmonic circular-membrane
/// modes, deep center hits, dry bright rim hits.
#[derive(Default)]
pub struct Damphu;

const FUNDAMENTAL_HZ: f32 = 90.0;
const MEMBRANE_RATIOS: [f32; 5] = [1.0, 1.59, 2.14, 2.30, 2.65];

fn voicing(stroke: Stroke, _velocity: f32) -> Voicing {
    match stroke {
        Stroke::Bass | Stroke::Open => {
            membrane_voicing(FUNDAMENTAL_HZ, &MEMBRANE_RATIOS, 0.40, -0.6, 700.0)
        }
        Stroke::Rim => membrane_voicing(FUNDAMENTAL_HZ * 2.1, &MEMBRANE_RATIOS, 0.12, 0.4, 3_500.0),
        Stroke::Slap | Stroke::Mute => {
            membrane_voicing(FUNDAMENTAL_HZ * 1.5, &MEMBRANE_RATIOS, 0.05, 0.2, 5_000.0)
        }
    }
}

impl Instrument for Damphu {
    fn name(&self) -> &str {
        "Damphu"
    }

    fn make_voice(&self, sample_rate: f32) -> Box<dyn Voice> {
        Box::new(ModalDrumVoice::new(sample_rate, 0xD4A7, voicing))
    }
}
