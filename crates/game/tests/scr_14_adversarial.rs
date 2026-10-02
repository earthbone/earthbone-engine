// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-14 registry checks for equipment and offset bindings.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_14_equipment_and_offset_bindings_are_actor_methods() {
    let game = Game::new(LoadOrder::new());
    for name in [
        "ClearKeepOffsetFromActor",
        "KeepOffsetFromActor",
        "SetForcedLandingMarker",
        "GetEquippedWeapon",
        "GetEquippedShield",
    ] {
        assert!(game.vm.natives.has_method("Actor", name), "Actor.{name}");
    }
}
