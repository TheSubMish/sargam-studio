use composer_core::model::{TempoMap, TICKS_PER_BEAT};

#[test]
fn constant_tempo_maps_linearly() {
    let map = TempoMap::constant(120.0);
    assert!((map.ticks_to_secs(0) - 0.0).abs() < 1e-6);
    assert!((map.ticks_to_secs(TICKS_PER_BEAT) - 0.5).abs() < 1e-4);
    assert!((map.ticks_to_secs(4 * TICKS_PER_BEAT) - 2.0).abs() < 1e-4);
}

#[test]
fn tempo_change_takes_effect_at_its_tick() {
    // 120 BPM for 4 beats (2.0 s), then 60 BPM: each later beat lasts 1.0 s.
    let mut map = TempoMap::constant(120.0);
    map.set_change(4 * TICKS_PER_BEAT, 60.0);

    assert!((map.ticks_to_secs(4 * TICKS_PER_BEAT) - 2.0).abs() < 1e-4);
    assert!((map.ticks_to_secs(5 * TICKS_PER_BEAT) - 3.0).abs() < 1e-4);
    assert!((map.ticks_to_secs(6 * TICKS_PER_BEAT) - 4.0).abs() < 1e-4);
    assert!((map.bpm_at(0) - 120.0).abs() < 1e-6);
    assert!((map.bpm_at(4 * TICKS_PER_BEAT) - 60.0).abs() < 1e-6);
    assert!((map.bpm_at(10 * TICKS_PER_BEAT) - 60.0).abs() < 1e-6);
}

#[test]
fn overwriting_a_change_replaces_it() {
    let mut map = TempoMap::constant(120.0);
    map.set_change(0, 90.0);
    assert_eq!(map.changes().len(), 1);
    assert!((map.bpm_at(0) - 90.0).abs() < 1e-6);
}
