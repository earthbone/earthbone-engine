// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{world::RngStream, World};

const SEED: u64 = 0xD15C_A11E_5EED_0001;
const STREAMS: [RngStream; 7] = [
    RngStream::Ai,
    RngStream::Dialogue,
    RngStream::ModData,
    RngStream::LeveledItems,
    RngStream::ActorTemplates,
    RngStream::ActorSpells,
    RngStream::BehaviorTree,
];

#[test]
fn tst_01_every_system_stream_is_independent_of_every_other_stream() {
    for target in STREAMS {
        let mut baseline = World::new_with_seed(LoadOrder::new(), SEED);
        let mut interleaved = World::new_with_seed(LoadOrder::new(), SEED);
        let expected = baseline.random_u64_for(target);

        for other in STREAMS {
            if other != target {
                for _ in 0..5 {
                    interleaved.random_u64_for(other);
                }
            }
        }

        assert_eq!(
            interleaved.random_u64_for(target),
            expected,
            "draws from other systems shifted {target:?}"
        );
    }
}

#[test]
fn tst_01_save_load_resumes_every_system_stream() {
    let mut original = World::new_with_seed(LoadOrder::new(), SEED);
    for (index, stream) in STREAMS.into_iter().enumerate() {
        for _ in 0..=index {
            original.random_u64_for(stream);
        }
    }
    let saved = original.save_state();
    let mut restored = World::new(LoadOrder::new());
    restored.load_state(&saved).unwrap();

    for stream in STREAMS {
        assert_eq!(
            original.random_u64_for(stream),
            restored.random_u64_for(stream),
            "save/load changed the next {stream:?} draw"
        );
    }
}
