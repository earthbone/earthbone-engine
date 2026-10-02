// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-15 registry checks for spell, voice, idle and crime bindings.

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_15_spell_voice_mount_and_trespass_bindings_are_actor_methods() {
    let game = Game::new(LoadOrder::new());
    for name in [
        "GetEquippedSpell",
        "GetVoiceRecoveryTime",
        "PlayIdleWithTarget",
        "Dismount",
        "Trespass",
    ] {
        assert!(game.vm.natives.has_method("Actor", name), "Actor.{name}");
    }
}
