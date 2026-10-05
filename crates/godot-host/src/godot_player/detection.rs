// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot physics sight queries feeding the shared detection updater.
//! PhysicsDirectSpaceState3D.intersect_ray reports the first body RID.

use std::collections::BTreeMap;

use godot::prelude::*;
use soe_esm::{GlobalFormId, Tag};
use soe_game::detection::Motion;
use soe_physics::PhysicsWorld;

use super::PlayerNode;
use crate::godot_combat::{ContactHit, ContactRays};
use crate::units::SkyrimPos;

#[godot_api(secondary)]
impl PlayerNode {
    /// Return enabled LIGH references in the active cell as FormIDs and Godot
    /// positions. The live detection check uses these to isolate record-backed
    /// light changes without creating actor scene nodes.
    #[func]
    pub fn detection_light_references(&mut self) -> Array<VarDictionary> {
        let mut result = Array::new();
        let Some(game) = &mut self.game else {
            return result;
        };
        let player = game.player();
        let world = game.world();
        let Some(cell) = world
            .reference_ref(player)
            .and_then(|reference| reference.cell)
        else {
            return result;
        };
        let references = world.light_references_in_cell(cell);
        for id in references {
            let (base, placement) = if let Some(record) = world
                .record(id)
                .filter(|record| record.tag == Tag(*b"REFR"))
            {
                let Some(raw_base) = record.find(Tag(*b"NAME")).filter(|bytes| bytes.len() == 4)
                else {
                    continue;
                };
                let raw = u32::from_le_bytes([raw_base[0], raw_base[1], raw_base[2], raw_base[3]]);
                (world.resolve_from(id, raw), world.ref_placement(id))
            } else if let Some(reference) = world.reference_ref(id) {
                let placement = soe_game::world::RefPlacement {
                    position: reference.position,
                    rotation: reference.rotation,
                    scale: reference.scale,
                    visible: reference.enabled && !reference.deleted,
                };
                (reference.base, Some(placement))
            } else {
                continue;
            };
            let Some(base) = base else { continue };
            if world
                .record(base)
                .is_none_or(|record| record.tag != Tag(*b"LIGH"))
            {
                continue;
            }
            let Some(placement) = placement.filter(|placement| placement.visible) else {
                continue;
            };
            let position = SkyrimPos::new(
                placement.position[0],
                placement.position[1],
                placement.position[2],
            )
            .to_godot();
            let mut row = VarDictionary::new();
            row.set("file", i64::from(id.file));
            row.set("object", i64::from(id.object));
            row.set("position", Vector3::new(position.x, position.y, position.z));
            result.push(&row);
        }
        result
    }

    /// Run one shared detection sample on demand. The gameplay loop calls the
    /// same path each physics step. This entry point lets host checks sample
    /// the live physics space at a controlled point in time.
    #[func]
    pub fn sample_detection(&mut self, elapsed: f64) -> i64 {
        self.advance_detection(elapsed, Motion::Still)
    }

    /// Walk the host capsule and stream its exterior before a detection sample.
    /// This harness holds NPC placements fixed and omits AI planning and VM
    /// ticks, while using the complete gameplay detection entry point.
    #[func]
    pub fn sample_detection_walk(&mut self, elapsed: f64, direction: Vector3) -> i64 {
        if !elapsed.is_finite() || elapsed <= 0.0 || !direction.is_finite() {
            return -1;
        }
        let (Some(physics), Some(character)) = (&mut self.world, self.character) else {
            return -1;
        };
        physics.walk(
            character,
            soe_physics::Vec3::new(direction.x, direction.y, direction.z),
            200.0,
        );
        physics.step(elapsed as f32);
        let feet = self.feet();
        let Some(game) = &mut self.game else {
            return -1;
        };
        let player = game.player();
        if let Some(reference) = game.world_mut().reference(player) {
            reference.position = [feet.x, feet.y, feet.z];
        }
        if !self.advance_exterior() {
            return -1;
        }
        self.advance_detection(elapsed, Motion::Walking)
    }

    /// Read one observer's shared detection value for a host integration check.
    #[func]
    pub fn detection_value(
        &self,
        observer_file: i32,
        observer_object: i64,
        target_file: i32,
        target_object: i64,
    ) -> f64 {
        let (Ok(observer_file), Ok(observer_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(observer_file),
            u32::try_from(observer_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return -1.0;
        };
        let observer = GlobalFormId {
            file: observer_file,
            object: observer_object,
        };
        let target = GlobalFormId {
            file: target_file,
            object: target_object,
        };
        self.game
            .as_ref()
            .and_then(|game| game.world().awareness(observer, target))
            .map_or(-1.0, |value| f64::from(value.detection))
    }

    /// Whether the shared detection state has an observer entry for this pair.
    #[func]
    pub fn has_detection_awareness(
        &self,
        observer_file: i32,
        observer_object: i64,
        target_file: i32,
        target_object: i64,
    ) -> bool {
        let (Ok(observer_file), Ok(observer_object), Ok(target_file), Ok(target_object)) = (
            u16::try_from(observer_file),
            u32::try_from(observer_object),
            u16::try_from(target_file),
            u32::try_from(target_object),
        ) else {
            return false;
        };
        let observer = GlobalFormId {
            file: observer_file,
            object: observer_object,
        };
        let target = GlobalFormId {
            file: target_file,
            object: target_object,
        };
        self.game
            .as_ref()
            .is_some_and(|game| game.world().awareness(observer, target).is_some())
    }

    /// Queue a sound from an actor reference for the next shared detection step.
    #[func]
    pub fn emit_detection_sound(&mut self, file: i32, object: i64, sound_level: i32) -> bool {
        self.emit_detection_sound_at(file, object, file, object, sound_level)
    }

    /// Queue an actor-owned sound at another reference, following the CK
    /// CreateDetectionEvent source and owner contract for host checks.
    #[func]
    pub fn emit_detection_sound_at(
        &mut self,
        source_file: i32,
        source_object: i64,
        owner_file: i32,
        owner_object: i64,
        sound_level: i32,
    ) -> bool {
        let (Ok(source_file), Ok(source_object)) =
            (u16::try_from(source_file), u32::try_from(source_object))
        else {
            return false;
        };
        let (Ok(file), Ok(object)) = (u16::try_from(owner_file), u32::try_from(owner_object))
        else {
            return false;
        };
        let source = GlobalFormId {
            file: source_file,
            object: source_object,
        };
        let owner = GlobalFormId { file, object };
        self.game.as_mut().is_some_and(|game| {
            game.world_mut()
                .create_detection_event(source, owner, sound_level)
        })
    }

    /// Find an enabled unlocked door from the current exterior worldspace
    /// to Tamriel, across that worldspace's cells, for the gate walking check.
    #[func]
    pub fn detection_gate_reference(&mut self) -> VarDictionary {
        let mut row = VarDictionary::new();
        let (Some(exterior), Some(source), Some(game)) =
            (&self.exterior, &self.data_source, &mut self.game)
        else {
            return row;
        };
        let mut cells = source
            .cell_index()
            .cells()
            .map(|(cell, _)| cell)
            .filter(|cell| source.exterior_world(*cell) == Some(exterior.world))
            .collect::<Vec<_>>();
        cells.sort_unstable();
        let mut world = game.world_mut();
        for cell in cells {
            for (id, far) in world.cell_doors(cell) {
                let Some(reference) = world.reference(id).filter(|reference| {
                    reference.enabled && !reference.deleted && !reference.locked
                }) else {
                    continue;
                };
                let position = reference.position;
                let Some(far_cell) = far
                    .and_then(|far| world.reference(far))
                    .and_then(|reference| reference.cell)
                else {
                    continue;
                };
                let Some(far_world) = source.exterior_world(far_cell) else {
                    continue;
                };
                if world
                    .record(far_world)
                    .and_then(|record| record.editor_id())
                    .as_deref()
                    != Some("Tamriel")
                {
                    continue;
                }
                row.set("file", i64::from(id.file));
                row.set("object", i64::from(id.object));
                row.set(
                    "position",
                    Vector3::new(position[0], position[1], position[2]),
                );
                return row;
            }
        }
        row
    }

    /// Current exterior worldspace's editor ID for live detection checks.
    #[func]
    pub fn detection_worldspace(&self) -> GString {
        let (Some(exterior), Some(game)) = (&self.exterior, &self.game) else {
            return GString::new();
        };
        game.world()
            .record(exterior.world)
            .and_then(|record| record.editor_id())
            .map_or_else(GString::new, |name| GString::from(&name))
    }

    /// Microseconds spent in the last complete detection update, including
    /// actor body synchronization, neighborhood sampling and world evaluation.
    #[func]
    pub fn detection_time_usec(&self) -> i64 {
        self.detection_time_usec
    }

    /// Last update's body sync, light construction, observation and evaluation
    /// costs in microseconds, for the PRF-31 stage measurement.
    #[func]
    pub fn detection_stage_times(&self) -> PackedInt64Array {
        PackedInt64Array::from(self.detection_stage_usec.as_slice())
    }

    pub(super) fn advance_detection(&mut self, elapsed: f64, player_motion: Motion) -> i64 {
        let started = std::time::Instant::now();
        let result = self.advance_detection_step(elapsed, player_motion);
        self.detection_time_usec = i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX);
        result
    }

    fn advance_detection_step(&mut self, elapsed: f64, player_motion: Motion) -> i64 {
        let _system = soe_trace::SystemScope::new(soe_trace::System::AiPlanning);
        if !elapsed.is_finite() || elapsed < 0.0 {
            return -1;
        }
        let Some(game) = &mut self.game else {
            return -1;
        };
        let player = game.player();
        let range = match game.world().detection_settings() {
            Ok(settings) => settings.max_distance,
            Err(error) => {
                godot_error!("PlayerNode detection: {error}");
                return -1;
            }
        };
        let mut shared = game.world_mut();
        let events = shared
            .take_detection_events()
            .into_iter()
            .filter(|event| {
                shared
                    .actor_ref(event.owner)
                    .is_none_or(|state| !state.dead)
            })
            .collect::<Vec<_>>();
        drop(shared);
        let started = std::time::Instant::now();
        let mut spaces = None;
        let Some(actors) = self.sync_actor_bodies_and_contacts_with(|world, mut actors| {
            actors.retain(|actor| world.actor_ref(actor.actor).is_none_or(|state| !state.dead));
            let frame = crate::detection::DetectionSpaces::new(world, &actors, &events, range);
            let candidates = frame.candidate_actors(actors, &events);
            spaces = Some(frame);
            candidates
        }) else {
            return -1;
        };
        self.detection_stage_usec[0] = started.elapsed().as_micros() as i64;
        let Some(spaces) = spaces else { return -1 };
        let Some(space) = self.base().get_world_3d().map(|world| world.get_space()) else {
            return -1;
        };
        let Some(game) = &mut self.game else {
            return -1;
        };
        let mut motions = BTreeMap::new();
        motions.insert(player, player_motion);
        let owners = [&self.melee_bodies, self.actor_bodies.owners()];
        let world = game.world();
        let cells = actors.iter().map(|actor| actor.cell).collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let lights = crate::detection::LightSamples::new(&world, &cells);
        self.detection_stage_usec[1] = started.elapsed().as_micros() as i64;
        let started = std::time::Instant::now();
        let Ok(mut rays) = ContactRays::new(space, &owners) else {
            return -1;
        };
        let observations = spaces.observations(
            &actors,
            &events,
            &motions,
            |observer, target| {
                let position = |actor: &soe_game::world::ContactActor| {
                    let p = actor.position;
                    let eye = SkyrimPos::new(p[0], p[1], p[2] + 100.0 * actor.scale).to_godot();
                    Vector3::new(eye.x, eye.y, eye.z)
                };
                match rays.first_contact(observer.actor, position(observer), position(target)) {
                    Ok(ContactHit::Empty) => true,
                    Ok(ContactHit::Actor(id)) => id == target.actor,
                    _ => false,
                }
            },
            |cell, point| lights.level(cell, point),
        );
        self.detection_stage_usec[2] = started.elapsed().as_micros() as i64;
        drop(world);
        let observation_count = observations.len() as i64;
        let started = std::time::Instant::now();
        let result = if let Err(error) = game
            .world_mut()
            .evaluate_detection(&observations, elapsed as f32)
        {
            soe_trace::error!(target: "host::detection", system = "host::detection", "PlayerNode detection: {error}");
            -1
        } else {
            observation_count
        };
        self.detection_stage_usec[3] = started.elapsed().as_micros() as i64;
        result
    }
}
