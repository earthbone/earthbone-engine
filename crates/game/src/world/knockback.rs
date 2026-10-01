// SPDX-License-Identifier: GPL-3.0-or-later

//! Scripted knock impulses are queued for resident host ragdolls. CK pages
//! document PushActorAway's force argument and KnockAreaEffect's magnitude
//! and radius, but do not publish an area falloff formula.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::save::FormReader;
use super::{ActorImpulse, ActorState, World};

impl World {
    /// Queue a source-to-target impulse in Skyrim-space units.
    pub fn push_actor_away(
        &mut self,
        source: GlobalFormId,
        target: GlobalFormId,
        force: f32,
    ) -> Result<(), &'static str> {
        if !force.is_finite() {
            return Err("non-finite force");
        }
        let source = self.reference(source).ok_or("missing source")?.position;
        let target_position = self.reference(target).ok_or("missing target")?.position;
        if !self
            .reference(target)
            .is_some_and(|reference| reference.is_actor)
        {
            return Err("target is not an actor");
        }
        let vector = radial_impulse(source, target_position, force);
        self.queue_actor_impulse(target, vector);
        Ok(())
    }

    /// Queue area impulses for built actor references inside the source radius.
    /// The linear attenuation is an engine approximation because the CK page
    /// names no falloff curve.
    pub fn knock_area_effect(
        &mut self,
        source: GlobalFormId,
        magnitude: f32,
        radius: f32,
    ) -> Result<(), &'static str> {
        if !magnitude.is_finite() || !(0.0..=1.0).contains(&magnitude) {
            return Err("magnitude must be between zero and one");
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err("radius must be positive and finite");
        }
        let origin = self.reference(source).ok_or("missing source")?.position;
        let mut targets: Vec<_> = self
            .refs
            .iter()
            .filter_map(|(id, reference)| {
                if !reference.is_actor || !reference.enabled || reference.deleted || *id == source {
                    return None;
                }
                let dx = reference.position[0] - origin[0];
                let dy = reference.position[1] - origin[1];
                let dz = reference.position[2] - origin[2];
                let distance = (dx * dx + dy * dy + dz * dz).sqrt();
                (distance <= radius).then_some((*id, reference.position, distance))
            })
            .collect();
        targets.sort_by_key(|(id, _, _)| *id);
        for (actor, position, distance) in targets {
            let force = area_falloff(magnitude, distance, radius);
            self.queue_actor_impulse(actor, radial_impulse(origin, position, force));
        }
        Ok(())
    }

    /// Force the host representation to use or remove a ragdoll.
    pub fn force_ragdoll(
        &mut self,
        actor: GlobalFormId,
        enabled: bool,
    ) -> Result<(), &'static str> {
        let state = self.actor(actor).ok_or("reference is not an actor")?;
        state.ragdoll_forced = Some(enabled);
        Ok(())
    }

    /// Take all queued host impulses for one actor.
    pub fn take_actor_impulses(&mut self, actor: GlobalFormId) -> Vec<ActorImpulse> {
        let mut selected = Vec::new();
        self.actor_impulses.retain(|impulse| {
            if impulse.actor == actor {
                selected.push(*impulse);
                false
            } else {
                true
            }
        });
        selected
    }

    fn queue_actor_impulse(&mut self, actor: GlobalFormId, vector: [f32; 3]) {
        if vector.iter().all(|component| *component == 0.0) {
            return;
        }
        if let Some(state) = self.actor(actor) {
            state.unconscious = true;
        }
        self.actor_impulses.push(ActorImpulse { actor, vector });
    }
}

fn radial_impulse(source: [f32; 3], target: [f32; 3], force: f32) -> [f32; 3] {
    let delta = [
        target[0] - source[0],
        target[1] - source[1],
        target[2] - source[2],
    ];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, force]
    } else {
        [
            delta[0] * force / length,
            delta[1] * force / length,
            delta[2] * force / length,
        ]
    }
}

fn area_falloff(magnitude: f32, distance: f32, radius: f32) -> f32 {
    magnitude * (1.0 - distance / radius)
}

pub(super) fn write_ragdoll_override(actor: &ActorState, writer: &mut Writer) {
    writer.u8(match actor.ragdoll_forced {
        None => 0,
        Some(false) => 1,
        Some(true) => 2,
    });
}

pub(super) fn read_ragdoll_override(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    actor.ragdoll_forced = match reader.u8()? {
        0 => None,
        1 => Some(false),
        2 => Some(true),
        value => {
            return Err(VmError::State(format!(
                "invalid CMB-38 ragdoll mode {value}"
            )))
        }
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world_with_actors() -> World {
        let mut world = World::new(soe_esm::LoadOrder::new());
        for (form, position, is_actor) in [
            (id(1), [0.0, 0.0, 0.0], false),
            (id(2), [3.0, 4.0, 0.0], true),
            (id(3), [100.0, 0.0, 0.0], true),
        ] {
            let mut reference = super::super::RefState::new(None);
            reference.position = position;
            reference.is_actor = is_actor;
            world.refs.insert(form, reference);
            if is_actor {
                world.actors.insert(form, ActorState::default());
            }
        }
        world
    }

    #[test]
    fn cmb_38_push_actor_away_queues_signed_impulse_and_knockdown() {
        let mut world = world_with_actors();
        world.push_actor_away(id(1), id(2), -10.0).unwrap();
        assert_eq!(
            world.take_actor_impulses(id(2))[0].vector,
            [-6.0, -8.0, 0.0]
        );
        let actor = world.actor_ref(id(2)).unwrap();
        assert!(actor.unconscious);
        assert_eq!(actor.ragdoll_forced, None);
    }

    #[test]
    fn cmb_38_area_effect_queues_only_in_radius_with_linear_attenuation() {
        let mut world = world_with_actors();
        world.knock_area_effect(id(1), 0.8, 10.0).unwrap();
        let impulses = world.take_actor_impulses(id(2));
        assert_eq!(impulses.len(), 1);
        for (actual, expected) in impulses[0].vector.into_iter().zip([0.24, 0.32, 0.0]) {
            assert!((actual - expected).abs() < 1e-6);
        }
        assert!(world.take_actor_impulses(id(3)).is_empty());
        assert!(world.actor_ref(id(2)).unwrap().unconscious);
    }

    #[test]
    fn cmb_38_ragdoll_override_encodes_each_mode() {
        for (mode, expected) in [(None, 0), (Some(false), 1), (Some(true), 2)] {
            let actor = ActorState {
                ragdoll_forced: mode,
                ..ActorState::default()
            };
            let mut writer = Writer::new();
            write_ragdoll_override(&actor, &mut writer);
            assert_eq!(writer.out, [expected]);
        }
    }

    #[test]
    fn cmb_38_force_ragdoll_add_and_remove_update_actor_state() {
        let mut world = world_with_actors();
        world.force_ragdoll(id(2), true).unwrap();
        assert_eq!(world.actor_ref(id(2)).unwrap().ragdoll_forced, Some(true));
        assert!(!world.actor_ref(id(2)).unwrap().unconscious);
        world.force_ragdoll(id(2), false).unwrap();
        assert_eq!(world.actor_ref(id(2)).unwrap().ragdoll_forced, Some(false));
        assert!(!world.actor_ref(id(2)).unwrap().unconscious);
    }
}
