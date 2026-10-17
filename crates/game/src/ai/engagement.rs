// SPDX-License-Identifier: GPL-3.0-or-later

//! Actor engagement from xEdit NPC_ AIDT and FACT XNAM and the authored
//! aggroWarningStart and aggroWarningStop behavior graph events.

use soe_esm::{cursor::Fields, Error, GlobalFormId, Tag};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use crate::npc::{template, AiData};
use crate::world::save::{read_form_id, write_form_id, FormReader};
use crate::world::{reaction, ActorState};
use crate::World;

/// A neutral target warned through the actor's authored animation graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AggroWarning {
    pub target: GlobalFormId,
    pub completed: bool,
}

pub(super) struct Policy {
    aggression: f32,
    data: Option<AiData>,
}

impl Policy {
    pub(super) fn for_actor(world: &mut World, actor: GlobalFormId) -> Option<Self> {
        let aggression = world.actor_value(actor, "aggression");
        if aggression < 1.0 && world.is_calmed(actor) {
            return None;
        }
        let data = world.actor_ai_data(actor);
        if aggression < 1.0 && !data.is_some_and(|data| data.aggro_radius) {
            return None;
        }
        if world.actor_value(actor, "confidence") <= 0.0 {
            return None;
        }
        Some(Self { aggression, data })
    }

    pub(super) fn perceives(&self, awareness: crate::detection::Awareness) -> bool {
        awareness.level == crate::detection::DetectionLevel::Combat
            || (self.data.is_some_and(|data| data.aggro_radius)
                && awareness.level == crate::detection::DetectionLevel::Alert
                && awareness.detection > 0.0)
    }

    pub(super) fn refresh(&self, world: &mut World, actor: GlobalFormId) {
        let Some(warning) = world.actor_ref(actor).and_then(|a| a.aggro_warning) else {
            return;
        };
        let distance = world
            .reference_ref(actor)
            .zip(world.reference_ref(warning.target))
            .map(|(actor, target)| super::distance(actor.position, target.position));
        let valid = self.data.is_some_and(|data| {
            data.aggro_radius
                && distance.is_some_and(|distance| distance <= data.warn_radius as f32)
        }) && world
            .actor_ref(warning.target)
            .is_some_and(|target| !target.dead);
        if !valid {
            clear(world, actor);
        }
    }

    pub(super) fn accepts(
        &self,
        world: &mut World,
        actor: GlobalFormId,
        target: GlobalFormId,
        distance: f32,
    ) -> bool {
        let provoked = world
            .actor_ref(target)
            .is_some_and(|target| target.in_combat && target.combat_target == Some(actor))
            || world
                .actor_ref(actor)
                .is_some_and(|actor| actor.attack_on_sight);
        if provoked || self.aggression >= 3.0 || world.is_frenzied(actor) {
            return true;
        }
        let relation = relation(world, actor, target);
        if relation == reaction::ALLY || relation == reaction::FRIEND {
            return false;
        }
        let guards_area = world.actor_ref(actor).is_some_and(|state| {
            state.ai.procedure == super::Procedure::Guard
                && state.ai.center.is_some()
                && state.ai.radius > 0.0
        });
        if guards_area && !self.data.is_some_and(|data| data.aggro_radius) {
            return true;
        }
        if let Some(data) = self.data.filter(|data| data.aggro_radius) {
            if distance <= data.attack_radius as f32 {
                return true;
            }
            if distance > data.warn_radius as f32 {
                return false;
            }
            let prior = world.actor_ref(actor).and_then(|actor| actor.aggro_warning);
            if prior.is_some_and(|warning| warning.target != target) {
                return false;
            }
            if prior.is_none() {
                if let Some(actor) = world.loaded_actor_mut(actor) {
                    actor.aggro_warning = Some(AggroWarning {
                        target,
                        completed: false,
                    });
                    actor.alerted = true;
                }
                world.request_animation(actor, "aggroWarningStart");
                match aware_sounds(world, actor) {
                    Ok(sounds) => {
                        let total: u64 = sounds.iter().map(|(_, chance)| u64::from(*chance)).sum();
                        if total > 0 {
                            let mut draw =
                                world.random_u64_for(crate::world::RngStream::Ai) % total.max(100);
                            for (sound, chance) in sounds {
                                if draw < u64::from(chance) {
                                    let draws = std::array::from_fn(|_| {
                                        world.random_float_for(
                                            crate::world::RngStream::Ai,
                                            0.0,
                                            1.0,
                                        )
                                    });
                                    world.actor_sound_requests.push(
                                        crate::world::ActorSoundRequest {
                                            actor,
                                            descriptor: sound,
                                            draws,
                                        },
                                    );
                                    break;
                                }
                                draw -= u64::from(chance);
                            }
                        }
                    }
                    Err(error) => {
                        soe_trace::error!(target: "game::ai", system = "audio", "warning sound {actor}: {error}");
                    }
                }
            }
            return distance <= data.warn_attack_radius as f32
                && prior.is_some_and(|warning| warning.completed);
        }
        relation == reaction::ENEMY || self.aggression >= 2.0
    }
}

fn relation(world: &mut World, actor: GlobalFormId, target: GlobalFormId) -> i32 {
    let mut from: Vec<_> = world
        .actor_ref(actor)
        .into_iter()
        .flat_map(|actor| &actor.factions)
        .filter(|(_, rank)| **rank >= 0)
        .map(|(id, _)| *id)
        .collect();
    let mut to: Vec<_> = world
        .actor_ref(target)
        .into_iter()
        .flat_map(|actor| &actor.factions)
        .filter(|(_, rank)| **rank >= 0)
        .map(|(id, _)| *id)
        .collect();
    from.sort_unstable();
    to.sort_unstable();
    let target_is_player = world.is_player(target);
    let mut standing = reaction::NEUTRAL;
    for faction in from {
        let Some(state) = world.faction(faction) else {
            continue;
        };
        if target_is_player && (state.player_enemy || state.player_attacked) {
            return reaction::ENEMY;
        }
        for other in &to {
            let reaction = if faction == *other {
                reaction::ALLY
            } else {
                state
                    .reactions
                    .get(other)
                    .copied()
                    .unwrap_or(reaction::NEUTRAL)
            };
            if reaction == reaction::ENEMY {
                return reaction;
            }
            if matches!(reaction, reaction::ALLY | reaction::FRIEND) {
                standing = reaction;
            }
        }
    }
    let rank = world
        .actor_ref(actor)
        .and_then(|actor| actor.relationships.get(&target).copied())
        .or_else(|| {
            let base = world.reference_ref(actor)?.base?;
            let other = world.reference_ref(target)?.base?;
            world
                .relationship_record_rank(base, other)
                .or_else(|| world.relationship_record_rank(other, base))
        });
    match rank {
        Some(rank) if rank <= -3 => reaction::ENEMY,
        Some(rank) if rank >= 1 => reaction::FRIEND,
        _ => standing,
    }
}

pub(crate) fn provoke(world: &mut World, actor: GlobalFormId, attacker: GlobalFormId) {
    if actor == attacker || world.actor_value(actor, "confidence") <= 0.0 {
        return;
    }
    let eligible = world
        .actor_ref(actor)
        .is_some_and(|state| !state.dead && !state.unconscious && !state.in_combat);
    if !eligible {
        return;
    }
    clear(world, actor);
    if let Some(state) = world.loaded_actor_mut(actor) {
        state.in_combat = true;
        state.combat_target = Some(attacker);
        state.alerted = true;
        state.ai.path.clear();
        state.ai.path_cells.clear();
        state.ai.destination = None;
        state.ai.arrived = false;
    }
    world.ai.planner.cancel(actor);
    world.ai.combat_routes.remove(&actor);
}

pub(super) fn clear(world: &mut World, actor: GlobalFormId) {
    if world
        .loaded_actor_mut(actor)
        .is_some_and(|actor| actor.aggro_warning.take().is_some())
    {
        world.request_animation(actor, "aggroWarningStop");
    }
}

// NPC sounds use the eight categories in UESP NPC.wikitext and CommonLib's
// CreatureSounds, rather than xEdit's shared sound enum with four foot slots.
fn aware_sounds(world: &mut World, actor: GlobalFormId) -> Result<Vec<(GlobalFormId, u8)>, Error> {
    let mut owner = world.actor_component_source(actor, template::TRAITS)?;
    let mut seen = std::collections::BTreeSet::new();
    loop {
        if !seen.insert(owner) || seen.len() > 32 {
            return Err(Error::Invalid("cyclic actor audio template".into()));
        }
        let record = world
            .record(owner)
            .filter(|record| record.tag == Tag(*b"NPC_") && !record.is_deleted())
            .ok_or_else(|| Error::Invalid("missing actor audio template".into()))?;
        if let Some(data) = record.find(Tag(*b"CSCR")) {
            let mut field = Fields::new(record.tag, Tag(*b"CSCR"), data);
            field.exact(4)?;
            let raw = field.u32()?;
            if raw != 0 {
                owner = world
                    .resolve_from(owner, raw)
                    .ok_or_else(|| Error::Invalid("unresolved actor audio template".into()))?;
                continue;
            }
        }
        let mut category = None;
        let mut pending = None;
        let mut sounds = Vec::new();
        for sub in &record.subrecords {
            match sub.tag.as_bytes() {
                b"CSDT" => {
                    if pending.is_some() {
                        return Err(Error::Invalid("actor sound has no chance".into()));
                    }
                    let mut field = Fields::new(record.tag, sub.tag, &sub.data);
                    field.exact(4)?;
                    category = Some(field.u32()?);
                }
                b"CSDI" if category == Some(1) => {
                    if pending.is_some() {
                        return Err(Error::Invalid("actor sound has no chance".into()));
                    }
                    let mut field = Fields::new(record.tag, sub.tag, &sub.data);
                    field.exact(4)?;
                    let raw = field.u32()?;
                    pending = Some(if raw == 0 {
                        None
                    } else {
                        Some(
                            world
                                .resolve_from(owner, raw)
                                .ok_or_else(|| Error::Invalid("unresolved actor sound".into()))?,
                        )
                    });
                }
                b"CSDC" if category == Some(1) => {
                    let mut field = Fields::new(record.tag, sub.tag, &sub.data);
                    field.exact(1)?;
                    let chance = field.u8()?;
                    if chance > 100 {
                        return Err(Error::Invalid("actor sound chance exceeds 100".into()));
                    }
                    let sound = pending.take().ok_or_else(|| {
                        Error::Invalid("actor sound chance has no descriptor".into())
                    })?;
                    if let Some(sound) = sound {
                        sounds.push((sound, chance));
                    }
                }
                _ => {}
            }
        }
        if pending.is_some() {
            return Err(Error::Invalid("actor sound has no chance".into()));
        }
        return Ok(sounds);
    }
}

pub(crate) fn write_warning(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(actor.aggro_warning.is_some());
    if let Some(warning) = actor.aggro_warning {
        write_form_id(writer, warning.target);
        writer.bool(warning.completed);
    }
}

pub(crate) fn read_warning(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.aggro_warning = if reader.bool()? {
        Some(AggroWarning {
            target: read_form_id(reader)?,
            completed: reader.bool()?,
        })
    } else {
        None
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::detection::{Awareness, DetectionLevel, StealthPool};
    use crate::World;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn fixture(radius: bool, distance: f32) -> (World, GlobalFormId, GlobalFormId) {
        fixture_style(radius, distance, 1.0)
    }

    fn fixture_style(
        radius: bool,
        distance: f32,
        unarmed: f32,
    ) -> (World, GlobalFormId, GlobalFormId) {
        fixture_audio(radius, distance, unarmed, 100)
    }

    fn fixture_audio(
        radius: bool,
        distance: f32,
        unarmed: f32,
        chance: u8,
    ) -> (World, GlobalFormId, GlobalFormId) {
        fixture_equipment_style(radius, distance, 1.0, unarmed, chance)
    }

    fn fixture_equipment_style(
        radius: bool,
        distance: f32,
        melee: f32,
        unarmed: f32,
        chance: u8,
    ) -> (World, GlobalFormId, GlobalFormId) {
        let mut plugin = Plugin::new(FileHeader::default());
        let mut aidt = vec![1, 4, 70, 1, 0, 0, u8::from(radius), 0];
        for r in [1024_u32, 750, 375] {
            aidt.extend_from_slice(&r.to_le_bytes());
        }
        let mut race = vec![0_u8; 128];
        for (offset, value) in [(16, 1.0_f32), (20, 1.0), (96, 60.0), (100, 512.0)] {
            race[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(Record::new(Tag(*b"RACE"), 0xa00).sub(b"DATA", race));
        let mut style = vec![0_u8; 40];
        style[12..16].copy_from_slice(&melee.to_le_bytes());
        style[28..32].copy_from_slice(&unarmed.to_le_bytes());
        plugin.push_top(Record::new(Tag(*b"CSTY"), 0xa01).sub(b"CSGD", style));
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0x800)
                .sub(b"AIDT", aidt)
                .sub(b"RNAM", 0xa00_u32.to_le_bytes())
                .sub(b"ZNAM", 0xa01_u32.to_le_bytes())
                .sub(b"CSCR", 0xa03_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"SNDR"), 0xa02));
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0xa03)
                .sub(b"CSDT", 1_u32.to_le_bytes())
                .sub(b"CSDI", 0xa02_u32.to_le_bytes())
                .sub(b"CSDC", [chance]),
        );
        plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(b"NAME", 0x800_u32.to_le_bytes()));
        plugin.push_top(Record::new(Tag(*b"ACHR"), 0x802).sub(b"NAME", 0x800_u32.to_le_bytes()));
        let mut weapon_data = vec![0_u8; 100];
        weapon_data[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        weapon_data[8..12].copy_from_slice(&1.0_f32.to_le_bytes());
        weapon_data[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
        weapon_data[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"WEAP"), 0x1f4)
                .sub(b"DATA", [0_u8; 10])
                .sub(b"DNAM", weapon_data),
        );
        plugin.push_top(Record::new(Tag(*b"FACT"), 0x900));
        plugin.push_top(Record::new(Tag(*b"FACT"), 0x901));
        let mut order = LoadOrder::new();
        order.add("Aggro.esm", plugin).unwrap();
        let mut world = World::new(order);
        let actor = id(0x801);
        let target = world.player();
        world.actor(actor).unwrap();
        world.actor(target).unwrap();
        world.reference(actor).unwrap().position = [0.0; 3];
        world.reference(target).unwrap().position = [distance, 0.0, 0.0];
        world.detection.pools.insert(
            target,
            StealthPool {
                points: 0.0,
                alert_wait: 0.0,
                observers: [(
                    actor,
                    Awareness {
                        level: DetectionLevel::Combat,
                        detection: 100.0,
                        lost_timer: 0.0,
                    },
                )]
                .into(),
            },
        );
        (world, actor, target)
    }

    #[test]
    fn warning_policy_chooses_the_nearest_target_before_queuing_events() {
        let (mut world, actor, target) = fixture(true, 900.0);
        let other = id(0x802);
        world.actor(other).unwrap();
        world.detection.pools.insert(
            other,
            StealthPool {
                points: 0.0,
                alert_wait: 0.0,
                observers: [(
                    actor,
                    Awareness {
                        level: DetectionLevel::Combat,
                        detection: 100.0,
                        lost_timer: 0.0,
                    },
                )]
                .into(),
            },
        );
        let order: Vec<_> = world.detection.pools.keys().copied().collect();
        assert_eq!(order.len(), 2);
        for (target, distance) in [(order[0], 900.0), (order[1], 800.0)] {
            world.reference(target).unwrap().position = [distance, 0.0, 0.0];
        }
        assert!(order.contains(&target));
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert_eq!(
            world.actor(actor).unwrap().aggro_warning.unwrap().target,
            order[1]
        );
    }

    #[test]
    fn warning_policy_keeps_a_neutral_actor_outside_its_authored_radii_peaceful() {
        let (mut world, actor, _) = fixture(true, 1500.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(!world.actor(actor).unwrap().in_combat);
    }

    #[test]
    fn warning_policy_plays_the_authored_warning_before_chasing_a_neutral_actor() {
        let (mut world, actor, _) = fixture(true, 900.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(world
            .animation_requests
            .iter()
            .any(|r| r.target == actor && r.animation == "aggroWarningStart"));
    }

    #[test]
    fn warning_policy_aggressive_actors_need_hostility_to_attack_neutral_targets() {
        let (mut world, actor, _) = fixture(false, 100.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn warning_policy_shared_factions_prevent_unprovoked_acquisition() {
        let (mut world, actor, target) = fixture(true, 100.0);
        for member in [actor, target] {
            world.actor(member).unwrap().factions.insert(id(0x900), 0);
        }
        assert!(!super::super::acquire_combat_target(&mut world, actor));
    }
    #[test]
    fn warning_policy_warn_attack_waits_for_the_authored_graph_to_finish() {
        let (mut world, actor, _) = fixture(true, 600.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert_eq!(world.animation_requests.len(), 1);
        world.animation_event(actor, "aggroWarningStop", &mut |_, _| {
            soe_papyrus::ObjectId(1)
        });
        assert!(super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn warning_policy_preserves_authored_guard_area_intrusion_behavior() {
        let (mut world, actor, _) = fixture(false, 100.0);
        let state = world.actor(actor).unwrap();
        state.ai.procedure = super::super::Procedure::Guard;
        state.ai.center = Some([0.0; 3]);
        state.ai.radius = 200.0;
        assert!(super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn warning_policy_visible_alert_target_warns_before_combat_detection() {
        let (mut world, actor, target) = fixture(true, 900.0);
        world
            .detection
            .pools
            .get_mut(&target)
            .unwrap()
            .observers
            .get_mut(&actor)
            .unwrap()
            .level = DetectionLevel::Alert;
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(world
            .animation_requests
            .iter()
            .any(|r| r.target == actor && r.animation == "aggroWarningStart"));
    }

    #[test]
    fn warning_policy_inner_attack_radius_engages_without_a_warning_delay() {
        let (mut world, actor, _) = fixture(true, 300.0);
        assert!(super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn warning_policy_leaving_the_radius_ends_the_warning() {
        let (mut world, actor, target) = fixture(true, 900.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        world.reference(target).unwrap().position[0] = 1500.0;
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(world.actor(actor).unwrap().aggro_warning.is_none());
    }

    #[test]
    fn warning_policy_target_provocation_bypasses_the_radius() {
        let (mut world, actor, target) = fixture(true, 1500.0);
        let target = world.actor(target).unwrap();
        target.in_combat = true;
        target.combat_target = Some(actor);
        assert!(super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn warning_policy_cowardly_confidence_does_not_start_an_unprovoked_attack() {
        let (mut world, actor, _) = fixture(true, 300.0);
        world.set_actor_value(actor, "confidence", 0.0).unwrap();
        assert!(!super::super::acquire_combat_target(&mut world, actor));
    }

    #[test]
    fn combat_approach_uses_race_reach_instead_of_waypoint_tolerance() {
        let (mut world, actor, target) = fixture(true, 600.0);
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(target);
        state.ai.speed = 300.0;
        state.ai.destination = Some([600.0, 0.0, 0.0]);
        state.ai.path = vec![[600.0, 0.0, 0.0]];
        for _ in 0..60 {
            super::super::pursue_combat_target(&mut world, actor, 1.0 / 60.0);
        }
        let position = world.reference(actor).unwrap().position;
        let remaining = 600.0 - position[0];
        assert!(
            (500.0..=512.0).contains(&remaining),
            "combat stop distance={remaining}"
        );
        assert!(world.actor(actor).unwrap().ai.arrived);
    }

    #[test]
    fn combat_approach_large_step_never_overshoots_reach() {
        let (mut world, actor, target) = fixture(true, 600.0);
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(target);
        state.ai.speed = 1000.0;
        state.ai.destination = Some([600.0, 0.0, 0.0]);
        state.ai.path = vec![[600.0, 0.0, 0.0]];
        super::super::pursue_combat_target(&mut world, actor, 1.0);
        let remaining = 600.0 - world.reference(actor).unwrap().position[0];
        assert!(
            (remaining - 512.0).abs() < 0.001,
            "combat stop distance={remaining}"
        );
    }

    #[test]
    fn combat_approach_scales_race_reach_by_live_reference_scale() {
        let (mut world, actor, _) = fixture(true, 1500.0);
        world.reference(actor).unwrap().scale = 2.0;
        assert_eq!(world.combat_approach_distance(actor).unwrap(), 1024.0);
    }

    #[test]
    fn combat_approach_uses_creature_melee_reach_with_zero_unarmed_preference() {
        let (mut world, actor, _) = fixture_style(true, 1500.0, 0.0);
        assert_eq!(world.combat_approach_distance(actor).unwrap(), 512.0);
    }

    #[test]
    fn combat_approach_honors_the_styles_disabled_unarmed_option() {
        let (mut world, actor, _) = fixture_equipment_style(true, 1500.0, 0.0, 0.0, 100);
        assert!(world.combat_approach_distance(actor).is_err());
    }

    #[test]
    fn combat_approach_rejects_a_non_finite_style_multiplier() {
        let (mut world, actor, _) = fixture_style(true, 1500.0, f32::NAN);
        assert!(world.combat_approach_distance(actor).is_err());
    }

    #[test]
    fn warning_policy_validated_hit_provokes_the_victim_outside_warning_range() {
        let (mut world, actor, attacker) = fixture(true, 1500.0);
        world.set_actor_value(actor, "health", 100.0).unwrap();
        world
            .set_actor_value(attacker, "unarmeddamage", 10.0)
            .unwrap();
        world
            .melee_hit(
                crate::combat::MeleeHit {
                    attacker,
                    target: actor,
                    weapon: id(0x1f4),
                    is_bash: false,
                    is_power_attack: false,
                },
                Vec::new(),
                Vec::new(),
                &mut |_, _| soe_papyrus::ObjectId(1),
            )
            .unwrap();
        assert!(world.actor_value(actor, "health") > 0.0);
        assert!(world.actor(actor).unwrap().in_combat);
        assert_eq!(world.actor(actor).unwrap().combat_target, Some(attacker));
    }

    #[test]
    fn warning_policy_queues_the_traits_audio_templates_aware_sound_once() {
        let (mut world, actor, _) = fixture(true, 900.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert_eq!(
            world
                .actor_sound_requests
                .iter()
                .map(|request| (request.actor, request.descriptor))
                .collect::<Vec<_>>(),
            [(actor, id(0xa02))]
        );
        assert!(world.actor_sound_requests[0]
            .draws
            .iter()
            .all(|draw| (0.0..=1.0).contains(draw)));
    }

    #[test]
    fn warning_policy_respects_an_authored_zero_sound_chance() {
        let (mut world, actor, _) = fixture_audio(true, 900.0, 1.0, 0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        assert!(world.actor_sound_requests.is_empty());
    }

    #[test]
    fn warning_policy_rejects_an_invalid_sound_chance() {
        let (mut world, actor, _) = fixture_audio(true, 900.0, 1.0, 255);
        assert!(super::aware_sounds(&mut world, actor).is_err());
    }

    #[test]
    fn warning_policy_named_save_section_retains_warning_target_and_completion() {
        let (mut world, actor, target) = fixture(true, 900.0);
        assert!(!super::super::acquire_combat_target(&mut world, actor));
        world.animation_event(actor, "aggroWarningStop", &mut |_, _| {
            soe_papyrus::ObjectId(1)
        });
        let bytes = world.save_state();
        let (mut loaded, _, _) = fixture(true, 900.0);
        loaded.load_state(&bytes).unwrap();
        assert_eq!(
            loaded.actor(actor).unwrap().aggro_warning,
            Some(super::AggroWarning {
                target,
                completed: true
            })
        );
    }
}
