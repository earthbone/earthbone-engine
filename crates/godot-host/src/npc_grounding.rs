// SPDX-License-Identifier: GPL-3.0-or-later

//! HST-30 transient NPC capsules over the player's installed cell collision.
//! Uses soe-physics' player controller. Godotwind's MoveContainer and WalkMove
//! separate horizontal intent from gravity and perform one controller update.
//! OpenMW AiWander::evadeObstacles retries obstructed wander destinations.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_physics::{BodyId, Character, CharacterSettings, PhysicsWorld, SimpleWorld, Vec3};

/// Maximum downward correction of an authored spawn, in Skyrim units.
pub const SPAWN_SNAP: f32 = 128.0;

/// Continuous blocked intent before a sandbox target is retried.
pub const BLOCKED_RETRY_SECONDS: f32 = 1.0;

/// Transient collision feedback for the current resident capsule.
#[derive(Debug, Clone, Copy)]
pub struct MovementDiagnostics {
    /// Latest requested placement, before controller collision resolution.
    pub target: [f32; 3],
    /// Continuous grounded intent with less than five percent actual progress.
    pub blocked_seconds: f32,
    /// Number of blocked controller steps since this capsule was installed.
    pub blocked_steps: u64,
}

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
        if hit.normal.z >= CharacterSettings::mudcrab().max_slope_cos
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
    blocked_seconds: f32,
    blocked_steps: u64,
}

/// Transient controller resources. Resolved positions belong to World's references.
#[derive(Default)]
pub struct NpcGrounding {
    bodies: BTreeMap<GlobalFormId, Body>,
}

#[derive(Clone, Copy)]
pub(crate) struct WaterMotion {
    pub height: f32,
    pub depth: f32,
    pub speed: f32,
}

pub(crate) struct Motion {
    pub target: [f32; 3],
    pub scale: f32,
    pub delta: f32,
    pub water: Option<WaterMotion>,
}

impl NpcGrounding {
    /// Read movement feedback without constructing an actor or capsule.
    pub fn movement_diagnostics(&self, actor: GlobalFormId) -> Option<MovementDiagnostics> {
        let body = self.bodies.get(&actor)?;
        Some(MovementDiagnostics {
            target: [body.last_target.x, body.last_target.y, body.last_target.z],
            blocked_seconds: body.blocked_seconds,
            blocked_steps: body.blocked_steps,
        })
    }

    #[cfg(any(test, feature = "godot"))]
    pub(crate) fn retry_started(&mut self, actor: GlobalFormId) {
        if let Some(body) = self.bodies.get_mut(&actor) {
            body.blocked_seconds = 0.0;
        }
    }

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
        self.update_motion(
            world,
            actor,
            collision_cell,
            &Motion {
                target,
                scale,
                delta,
                water: None,
            },
        )
    }

    pub(crate) fn update_motion(
        &mut self,
        world: &mut SimpleWorld,
        actor: GlobalFormId,
        collision_cell: Option<GlobalFormId>,
        motion: &Motion,
    ) -> Option<[f32; 3]> {
        let Motion {
            target,
            scale,
            delta,
            water,
        } = *motion;
        if !target.into_iter().all(f32::is_finite)
            || !scale.is_finite()
            || scale <= 0.0
            || !delta.is_finite()
            || delta < 0.0
        {
            return None;
        }
        let defaults = CharacterSettings::mudcrab();
        let settings = CharacterSettings {
            radius: defaults.radius * scale,
            height: defaults.height * scale,
            crouch_height: defaults.crouch_height * scale,
            step_height: defaults.step_height * scale,
            step_min_width: defaults.step_min_width * scale,
            collision_offset: defaults.collision_offset * scale,
            ground_snap: defaults.ground_snap * scale,
            ..defaults
        };
        if ![
            settings.radius,
            settings.height,
            settings.crouch_height,
            settings.step_height,
            settings.step_min_width,
            settings.collision_offset,
            settings.ground_snap,
        ]
        .into_iter()
        .all(|dimension| dimension.is_finite() && dimension > 0.0)
        {
            return None;
        }
        let Some(cell) = collision_cell.filter(|_| world.static_count() > 0) else {
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
                    blocked_seconds: 0.0,
                    blocked_steps: 0,
                },
            );
        }
        let body = self.bodies.get_mut(&actor)?;
        body.last_target = target;
        if delta > 0.0 {
            let character = world.character(body.id)?;
            let before = character.position;
            let difference = target - character.position;
            let desired = if recreate {
                Vec3::ZERO
            } else {
                Vec3::new(difference.x, difference.y, 0.0) * (1.0 / delta)
            };
            if let Some(water) = water {
                let desired = if desired.length() > water.speed {
                    desired.normalized() * water.speed
                } else {
                    desired
                };
                world.step_swimming(body.id, desired, water.height, water.depth, delta);
                body.blocked_seconds = 0.0;
                body.blocked_steps = 0;
            } else {
                world.move_character(body.id, desired);
                world.step_character(body.id, delta);
                let character = world.character(body.id)?;
                let intent = desired.x.hypot(desired.y) * delta;
                let progress =
                    (character.position.x - before.x).hypot(character.position.y - before.y);
                if character.on_ground && intent > 0.1 && progress < intent * 0.05 {
                    body.blocked_seconds += delta;
                    body.blocked_steps += 1;
                } else {
                    body.blocked_seconds = 0.0;
                }
            }
        }
        let position = world.character(body.id)?.position;
        Some([position.x, position.y, position.z])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_nif::types::BlockRef;
    use soe_physics::{Collider, Iso, Layer, MotionType, Shape};

    #[test]
    fn blocked_controller_intent_is_reported_and_clears_after_real_progress() {
        let mut world = SimpleWorld::new();
        for (center, half) in [
            ([0.0, 0.0, -1.0], Vec3::new(1024.0, 1024.0, 1.0)),
            ([128.0, 0.0, 100.0], Vec3::new(1.0, 200.0, 100.0)),
        ] {
            world.add_static(&Collider {
                node: BlockRef(0),
                node_name: String::new(),
                layer: Layer::STATIC,
                motion: MotionType::Fixed,
                mass: 0.0,
                transform: Iso::placement(center, [0.0; 3], 1.0),
                shape: Shape::Box { half_extents: half },
            });
        }
        let actor = GlobalFormId { file: 0, object: 1 };
        let cell = GlobalFormId { file: 0, object: 2 };
        let mut grounding = NpcGrounding::default();
        let mut position = [0.0; 3];
        let dt = 1.0 / 60.0;
        for _ in 0..240 {
            position = grounding
                .update(
                    &mut world,
                    actor,
                    Some(cell),
                    [position[0] + 80.1 * dt, 0.0, position[2]],
                    1.0,
                    dt,
                )
                .unwrap();
        }
        let feedback = grounding.movement_diagnostics(actor).unwrap();
        assert!(feedback.blocked_seconds >= BLOCKED_RETRY_SECONDS);
        assert!(feedback.blocked_steps >= 60);
        assert!(position[0] <= 98.0);
        let blocked_at = position[0];
        grounding.retry_started(actor);
        assert_eq!(
            grounding
                .movement_diagnostics(actor)
                .unwrap()
                .blocked_seconds,
            0.0
        );
        for _ in 0..60 {
            position = grounding
                .update(
                    &mut world,
                    actor,
                    Some(cell),
                    [position[0] - 80.1 * dt, 0.0, position[2]],
                    1.0,
                    dt,
                )
                .unwrap();
        }
        assert!(position[0] < blocked_at - 64.0);
        assert_eq!(
            grounding
                .movement_diagnostics(actor)
                .unwrap()
                .blocked_seconds,
            0.0
        );
    }
}
