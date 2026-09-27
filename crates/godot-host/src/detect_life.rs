// SPDX-License-Identifier: GPL-3.0-or-later

//! Host presentation requests for actors selected by active Detect Life effects.

use soe_esm::GlobalFormId;
use soe_game::world::magic::{EffectId, MagicError};
use soe_game::world::World;

use crate::units::{GodotPos, SkyrimPos};

/// An actor highlight request in Godot coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionPresentation {
    pub effect: EffectId,
    pub actor: GlobalFormId,
    pub position: GodotPos,
    /// Winning MGEF Hit Shader form, kept intact for the host effect renderer.
    pub hit_shader: Option<GlobalFormId>,
}

/// Build current presentation requests from active effects in the shared world.
pub fn presentations(world: &mut World) -> Result<Vec<DetectionPresentation>, MagicError> {
    Ok(world
        .detect_life_targets()?
        .into_iter()
        .map(|target| DetectionPresentation {
            effect: target.effect,
            actor: target.actor,
            position: SkyrimPos::new(target.position[0], target.position[1], target.position[2])
                .to_godot(),
            hit_shader: target.hit_shader,
        })
        .collect())
}
