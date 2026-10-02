// SPDX-License-Identifier: GPL-3.0-or-later

//! Host presentation requests for actors selected by active Detect Life effects.

use soe_esm::GlobalFormId;
use soe_game::world::magic::{EffectId, MagicError};
use soe_game::world::EffectShaderRequest;
use soe_game::world::World;
use std::collections::BTreeSet;

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

/// Reconcile active detections with the EffectShader renderer's request queue.
/// Each selected actor receives its authored MGEF Hit Shader until it leaves
/// the detection set. Results and requests are stable by shader and actor ID.
pub fn effect_shader_requests(
    world: &World,
    detections: &[DetectionPresentation],
    active: &mut BTreeSet<(GlobalFormId, GlobalFormId)>,
) -> Vec<EffectShaderRequest> {
    let next: BTreeSet<_> = detections
        .iter()
        .filter_map(|detection| {
            let shader = detection.hit_shader?;
            world
                .record(shader)
                .is_some_and(|record| record.tag == soe_esm::Tag::new(b"EFSH"))
                .then_some((shader, detection.actor))
        })
        .collect();
    let mut requests = Vec::new();
    for &(shader, target) in next.difference(active) {
        requests.push(EffectShaderRequest {
            shader,
            target,
            duration: -1.0,
            playing: true,
            appearance: world.effect_shader_appearance(shader),
        });
    }
    for &(shader, target) in active.difference(&next) {
        requests.push(EffectShaderRequest {
            shader,
            target,
            duration: 0.0,
            playing: false,
            appearance: world.effect_shader_appearance(shader),
        });
    }
    *active = next;
    requests
}
