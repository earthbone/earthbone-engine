// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_70_sound_registry_includes_every_shipped_vanilla_method() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    assert!(game.vm.natives.has_method("Sound", "PlayAndWait"));
    assert!(
        !game.vm.natives.has_method("Sound", "GetDescriptor"),
        "GetDescriptor is an SKSE SoundDescriptor extension"
    );
}
