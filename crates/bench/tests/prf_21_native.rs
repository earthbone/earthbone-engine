// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn prf_21_native_save_profile_has_world_vm_and_envelope_rows() {
    let game = Game::new(LoadOrder::new());
    let (bytes, sections) = game.save_state_profiled().unwrap();
    assert!(bytes.starts_with(soe_game::save::MAGIC));
    assert_eq!(
        sections
            .iter()
            .map(|section| section.name)
            .collect::<Vec<_>>(),
        ["world", "papyrus_vm", "envelope"]
    );
    assert!(sections.iter().all(|section| section.bytes > 0));
    assert!(sections.iter().all(|section| section.nanos > 0));
}
