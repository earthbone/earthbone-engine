// SPDX-License-Identifier: GPL-3.0-or-later

//! Host snapshot of active EffectShader lifetimes from the shared game world.

use soe_game::world::effect_shader::EffectShaderPlayback;
use soe_game::World;

/// Derived applications for host presentation. The game world owns playback.
#[derive(Default)]
pub struct EffectShaderStack {
    playbacks: Vec<EffectShaderPlayback>,
}

impl EffectShaderStack {
    /// Replace the cached view with the world's current playback set.
    pub fn sync(&mut self, world: &World) {
        self.playbacks = world.effect_shader_playbacks();
    }

    /// Return the last synchronized playback snapshot.
    #[must_use]
    pub fn playbacks(&self) -> &[EffectShaderPlayback] {
        &self.playbacks
    }
}
