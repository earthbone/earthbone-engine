// SPDX-License-Identifier: GPL-3.0-or-later

//! HST-30 transient NPC capsules over the player's installed cell collision.
//! Uses soe-physics' player controller. Godotwind's MoveContainer and WalkMove
//! separate horizontal intent from gravity and perform one controller update.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_physics::{BodyId, Character, CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};

/// Maximum downward correction of an authored spawn, in Skyrim units.
pub const SPAWN_SNAP: f32 = 128.0;

/// Nearest walkable collision surface at the feet's XY, within the spawn range.
/// Upper floors and ceilings do not replace a nearer supporting floor.
pub fn support_height(world: &SimpleWorld, position: Vec3) -> Option<f32> {
    let mut from = position + Vec3::Z * SPAWN_SNAP;
    let to = position - Vec3::Z * SPAWN_SNAP;
    let mut nearest: Option<f32> = None;
    for _ in 0..32 {
        let Some(hit) = world.raycast(from, to) else {
            break;
        };
        if hit.normal.z >= CharacterSettings::default().max_slope_cos
            && nearest
                .is_none_or(|height| (hit.point.z - position.z).abs() < (height - position.z).abs())
        {
            nearest = Some(hit.point.z);
        }
        from = hit.point - Vec3::Z * 0.01;
        if from.z <= to.z {
            break;
        }
    }
    nearest
}

struct Body {
    id: BodyId,
    cell: GlobalFormId,
    scale: f32,
    last_target: Vec3,
}

/// Transient controller resources. Resolved positions belong to World's references.
#[derive(Default)]
pub struct NpcGrounding {
    bodies: BTreeMap<GlobalFormId, Body>,
}

impl NpcGrounding {
    /// Forget handles when their entire collision world is replaced.
    pub fn reset(&mut self) {
        self.bodies.clear();
    }

    /// Release capsules whose actors are no longer eligible in resident collision.
    pub fn retain(&mut self, world: &mut SimpleWorld, actors: &BTreeSet<GlobalFormId>) {
        self.bodies.retain(|actor, body| {
            if actors.contains(actor) {
                true
            } else {
                world.remove(body.id);
                false
            }
        });
    }

    /// Read the capsule presenting an actor, without constructing gameplay state.
    pub fn character<'a>(
        &self,
        world: &'a SimpleWorld,
        actor: GlobalFormId,
    ) -> Option<&'a Character> {
        world.character(self.bodies.get(&actor)?.id)
    }

    /// Resolve a shared placement through gravity, stairs and floor collision.
    /// An absent collision cell removes the capsule and defers placement.
    /// Invalid positions, scales and time are rejected before changing a body.
    pub fn update(
        &mut self,
        world: &mut SimpleWorld,
        actor: GlobalFormId,
        collision_cell: Option<GlobalFormId>,
        target: [f32; 3],
        scale: f32,
        delta: f32,
    ) -> Option<[f32; 3]> {
        if !target.into_iter().all(f32::is_finite)
            || !scale.is_finite()
            || scale <= 0.0
            || !delta.is_finite()
            || delta < 0.0
        {
            return None;
        }
        let defaults = CharacterSettings::default();
        let settings = CharacterSettings {
            radius: defaults.radius * scale,
            height: defaults.height * scale,
            crouch_height: defaults.crouch_height * scale,
            step_height: defaults.step_height * scale,
            ..defaults
        };
        if ![
            settings.radius,
            settings.height,
            settings.crouch_height,
            settings.step_height,
        ]
        .into_iter()
        .all(|dimension| dimension.is_finite() && dimension > 0.0)
        {
            return None;
        }
        let Some(cell) = collision_cell else {
            if let Some(body) = self.bodies.remove(&actor) {
                world.remove(body.id);
            }
            return None;
        };
        let target = Vec3::new(target[0], target[1], target[2]);
        let recreate = self.bodies.get(&actor).is_none_or(|body| {
            body.cell != cell
                || body.scale != scale
                || world.character(body.id).is_none()
                || (body.last_target - target).length() > SPAWN_SNAP
        });
        if recreate {
            if let Some(body) = self.bodies.remove(&actor) {
                world.remove(body.id);
            }
            let position = world
                .raycast(target + Vec3::Z * 0.5, target - Vec3::Z * SPAWN_SNAP)
                .filter(|hit| hit.normal.z >= settings.max_slope_cos && hit.point.z <= target.z)
                .map_or(target, |hit| Vec3::new(target.x, target.y, hit.point.z));
            let id = world.create_character(settings, position);
            self.bodies.insert(
                actor,
                Body {
                    id,
                    cell,
                    scale,
                    last_target: target,
                },
            );
        }
        let body = self.bodies.get_mut(&actor)?;
        body.last_target = target;
        if delta > 0.0 {
            let character = world.character(body.id)?;
            let difference = target - character.position;
            let desired = if recreate {
                Vec3::ZERO
            } else {
                Vec3::new(difference.x, difference.y, 0.0) * (1.0 / delta)
            };
            let radius = character.settings.radius;
            world.move_character(body.id, desired);
            // CONTEXT: Substeps keep lateral moves shorter than the capsule radius
            // as well as bounding gravity integration on coarse AI updates.
            let steps = (delta / (1.0 / 60.0))
                .max(desired.length() * delta / (radius * 0.5))
                .ceil()
                .clamp(1.0, 4096.0) as u32;
            for _ in 0..steps {
                world.step_character(body.id, delta / steps as f32);
            }
        }
        let position = world.character(body.id)?.position;
        Some([position.x, position.y, position.z])
    }
}
