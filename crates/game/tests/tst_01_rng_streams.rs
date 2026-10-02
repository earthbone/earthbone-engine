// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::RngStream, World};

const SEED: u64 = 0x1234_5678_9ABC_DEF0;

#[test]
fn tst_01_same_seed_replays_each_system_stream() {
    let mut first = World::new_with_seed(LoadOrder::new(), SEED);
    let mut replay = World::new_with_seed(LoadOrder::new(), SEED);

    for stream in [RngStream::Ai, RngStream::Dialogue, RngStream::ModData] {
        let left = (0..4)
            .map(|_| first.random_u64_for(stream))
            .collect::<Vec<_>>();
        let right = (0..4)
            .map(|_| replay.random_u64_for(stream))
            .collect::<Vec<_>>();
        assert_eq!(left, right, "same seed must replay {stream:?}");
    }
}

#[test]
fn tst_01_different_seed_derives_different_streams() {
    let mut first = World::new_with_seed(LoadOrder::new(), SEED);
    let mut second = World::new_with_seed(LoadOrder::new(), SEED + 1);

    assert_ne!(
        first.random_u64_for(RngStream::Dialogue),
        second.random_u64_for(RngStream::Dialogue)
    );
}

#[test]
fn tst_01_new_system_draw_does_not_shift_other_streams() {
    let mut baseline = World::new_with_seed(LoadOrder::new(), SEED);
    let mut with_new_caller = World::new_with_seed(LoadOrder::new(), SEED);

    let expected_dialogue = baseline.random_u64_for(RngStream::Dialogue);
    with_new_caller.random_u64_for(RngStream::Ai);
    let actual_dialogue = with_new_caller.random_u64_for(RngStream::Dialogue);

    assert_eq!(actual_dialogue, expected_dialogue);
    assert_ne!(baseline.state_hash().rng, with_new_caller.state_hash().rng);
}

#[test]
fn tst_01_stream_states_resume_after_save_load() {
    let mut original = World::new_with_seed(LoadOrder::new(), SEED);
    original.random_u64_for(RngStream::Ai);
    original.random_u64_for(RngStream::Dialogue);
    let saved = original.save_state();

    let mut restored = World::new(LoadOrder::new());
    restored.load_state(&saved).unwrap();

    for stream in [RngStream::Ai, RngStream::Dialogue, RngStream::ModData] {
        assert_eq!(
            original.random_u64_for(stream),
            restored.random_u64_for(stream),
            "save/load must preserve {stream:?}"
        );
    }
}
