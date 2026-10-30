use composer_core::engine::render_song;
use composer_core::generate::generate;
use composer_core::model::Song;

#[test]
fn same_seed_same_song() {
    assert_eq!(generate(42).song.to_json(), generate(42).song.to_json());
}

#[test]
fn different_seeds_differ() {
    let a = generate(1).song.to_json();
    assert!((2..10).any(|s| generate(s).song.to_json() != a));
}

#[test]
fn every_style_is_reachable() {
    let mut genres = std::collections::HashSet::new();
    for seed in 0..60 {
        genres.insert(generate(seed).emoji);
    }
    assert_eq!(
        genres.len(),
        3,
        "all three taal styles should appear: {genres:?}"
    );
}

#[test]
fn songs_are_well_formed() {
    for seed in 0..40 {
        let g = generate(seed);
        let song = &g.song;
        assert!(!song.title.is_empty());
        assert_eq!(song.tracks.len(), 5);
        let secs = song.duration_secs();
        assert!((20.0..120.0).contains(&secs), "seed {seed}: {secs}s");
        for track in &song.tracks {
            assert!(
                !track.clips.is_empty(),
                "seed {seed}: {} has no clips",
                track.name
            );
            for clip in &track.clips {
                assert!(
                    clip.pattern < track.patterns.len(),
                    "seed {seed}: bad clip index"
                );
            }
            for pattern in &track.patterns {
                assert!(
                    !pattern.notes.is_empty(),
                    "seed {seed}: empty {}",
                    pattern.name
                );
                for n in &pattern.notes {
                    assert!(n.duration > 0);
                    assert!(
                        n.start + n.duration <= pattern.length,
                        "seed {seed}: note overruns {}",
                        pattern.name
                    );
                }
            }
        }
        // Round-trips through the on-disk project format.
        assert_eq!(&Song::from_json(&song.to_json()).unwrap(), song);
    }
}

#[test]
fn songs_render_audible_finite_audio() {
    for seed in [3, 17, 99] {
        let audio = render_song(&generate(seed).song, 22_050.0).expect("render");
        assert!(audio.iter().all(|s| s.is_finite()));
        let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            peak > 0.1,
            "seed {seed} rendered near-silence (peak {peak})"
        );
    }
}
