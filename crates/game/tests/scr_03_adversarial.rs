// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod acceptance {
    include!("scr_03_component.rs");

    #[cfg(test)]
    mod adversarial {
        use super::{fixture, wasm_component, ACTIVATOR, QUEST};
        use soe_esm::GlobalFormId;

        #[test]
        fn scr_03_invalid_component_does_not_poison_later_load() {
            let mut game = fixture();
            assert!(game.load_wasm_mod(b"not a wasm component").is_err());
            let bytes = std::fs::read(wasm_component()).unwrap();
            game.load_wasm_mod(&bytes).unwrap();
            assert!(game.activate(
                GlobalFormId { file: 0, object: ACTIVATOR },
                GlobalFormId { file: 0, object: ACTIVATOR },
            ));
            assert_eq!(game.world_mut().quest_state(GlobalFormId { file: 0, object: QUEST }).unwrap().current, 10);
        }

        #[test]
        fn scr_03_repeated_activation_keeps_stage_in_shared_quest_state() {
            let bytes = std::fs::read(wasm_component()).unwrap();
            let mut game = fixture();
            game.load_wasm_mod(&bytes).unwrap();
            let target = GlobalFormId { file: 0, object: ACTIVATOR };
            assert!(game.activate(target, target));
            assert!(game.activate(target, target));
            assert_eq!(game.world_mut().quest_state(GlobalFormId { file: 0, object: QUEST }).unwrap().current, 10);
        }
    }
}
