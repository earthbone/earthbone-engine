// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_70_sound_and_shader_bindings_keep_method_and_global_contracts() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    for (class, name) in [("Sound", "Play"), ("ShaderParticleGeometry", "Apply"), ("ShaderParticleGeometry", "Remove")] {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(!game.vm.natives.has_global(class, name), "{class}.{name}");
    }
    for name in ["StopInstance", "SetInstanceVolume"] {
        assert!(game.vm.natives.has_global("Sound", name), "Sound.{name}");
        assert!(!game.vm.natives.has_method("Sound", name), "Sound.{name}");
    }
}
