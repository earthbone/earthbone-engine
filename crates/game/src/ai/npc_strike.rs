// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary NPC strikes from xEdit NPC_/RACE ATKD, ATKE and ATKR.
//! OpenMW aicombat.cpp separates choosing a strike from its animation contact.

use crate::combat::{records::Weapon, Hand};
use crate::world::{
    save::{read_form_id, write_form_id, FormReader},
    ActorState,
};
use crate::World;
use soe_esm::{cursor::Fields, Error, GlobalFormId, Record, Tag};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

const UNARMED: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x1f4,
};

/// One selected graph strike. The host consumes its contact once, including misses.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcStrike {
    pub event: String,
    pub target: GlobalFormId,
    pub weapon: GlobalFormId,
    pub hand: Hand,
    pub contact_used: bool,
    pub finished: bool,
    pub ready_at: f64,
    pub recovery: f32,
}

struct Attack {
    event: String,
    chance: f32,
    recovery: f32,
    hand: Hand,
}

fn attacks(record: &Record) -> Result<Vec<Attack>, Error> {
    if record.is_deleted() || !matches!(record.tag.as_bytes(), b"NPC_" | b"RACE") {
        return Err(Error::Invalid("invalid attack source record".into()));
    }
    let mut pending = None;
    let mut out = Vec::new();
    for sub in &record.subrecords {
        match sub.tag.as_bytes() {
            b"ATKD" => {
                if pending.is_some() {
                    return Err(Error::Invalid("attack has no event".into()));
                }
                let mut data = Fields::new(record.tag, sub.tag, &sub.data);
                data.exact(44)?;
                let damage = data.f32()?;
                let chance = data.f32()?;
                let spell = data.u32()?;
                let flags = data.u32()?;
                let angle = data.f32()?;
                let strike = data.f32()?;
                data.skip(12)?;
                let recovery = data.f32()?;
                let stamina = data.f32()?;
                if [damage, chance, angle, strike, recovery, stamina]
                    .iter()
                    .any(|value| !value.is_finite())
                    || recovery < 0.0
                    || chance < 0.0
                {
                    return Err(Error::Invalid("invalid ordinary attack data".into()));
                }
                let eligible = damage == 1.0
                    && spell == 0
                    && flags & !8 == 0
                    && angle == 0.0
                    && strike > 0.0
                    && chance > 0.0;
                pending = Some((
                    eligible,
                    chance,
                    recovery,
                    if flags & 8 != 0 {
                        Hand::Left
                    } else {
                        Hand::Right
                    },
                ));
            }
            b"ATKE" => {
                let Some((eligible, chance, recovery, hand)) = pending.take() else {
                    return Err(Error::Invalid("attack event has no data".into()));
                };
                let event = soe_esm::cursor::zstring(&sub.data);
                if eligible && !event.is_empty() {
                    out.push(Attack {
                        event,
                        chance,
                        recovery,
                        hand,
                    });
                }
            }
            _ => {}
        }
    }
    if pending.is_some() {
        return Err(Error::Invalid("attack has no event".into()));
    }
    Ok(out)
}

fn actor_attacks(world: &mut World, actor: GlobalFormId) -> Result<Vec<Attack>, Error> {
    let mut owner = world.actor_component_source(actor, crate::npc::template::ATTACK_DATA)?;
    let race = world.actor_ref(actor).and_then(|state| state.race);
    let mut seen = std::collections::BTreeSet::new();
    loop {
        if !seen.insert(owner) {
            return Err(Error::Invalid("cyclic attack race".into()));
        }
        let record = world
            .record(owner)
            .ok_or_else(|| Error::Invalid("missing attack source".into()))?;
        let list = attacks(record)?;
        if record.find(Tag(*b"ATKD")).is_some() {
            return Ok(list);
        }
        let next = record
            .find(Tag(*b"ATKR"))
            .map(|bytes| {
                let mut data = Fields::new(record.tag, Tag(*b"ATKR"), bytes);
                data.exact(4)?;
                let raw = data.u32()?;
                if raw == 0 {
                    return Ok(None);
                }
                world
                    .resolve_from(owner, raw)
                    .map(Some)
                    .ok_or_else(|| Error::Invalid("unresolved attack race".into()))
            })
            .transpose()?
            .flatten();
        owner = match next.or_else(|| (record.tag == Tag(*b"NPC_")).then_some(race).flatten()) {
            Some(next) => next,
            None => return Ok(Vec::new()),
        };
    }
}

pub(super) fn start(world: &mut World, actor: GlobalFormId, target: GlobalFormId) {
    let now = world.clock.real_seconds;
    if world.actor_ref(actor).is_none_or(|state| {
        state.dead
            || state.unconscious
            || state.restrained
            || state.ai_disabled
            || !state.in_combat
            || state.combat_target != Some(target)
    }) {
        return;
    }
    if world
        .actor_ref(actor)
        .and_then(|state| state.npc_strike.as_ref())
        .is_some_and(|strike| !strike.finished || now < strike.ready_at)
    {
        return;
    }
    let options = match actor_attacks(world, actor) {
        Ok(options) => options,
        Err(error) => {
            soe_trace::error!(target:"game::ai",system="combat","strike {actor}: {error}");
            return;
        }
    };
    let options: Vec<_> = options
        .into_iter()
        .filter_map(|attack| {
            let weapon = world.hand_weapon(actor, attack.hand).unwrap_or(UNARMED);
            if weapon != UNARMED
                && world
                    .record(weapon)
                    .and_then(|record| Weapon::from_record(record).ok())
                    .is_none_or(|weapon| weapon.animation > 6)
            {
                return None;
            }
            Some((attack, weapon))
        })
        .collect();
    let total: f32 = options.iter().map(|(attack, _)| attack.chance).sum();
    if total <= 0.0 || !total.is_finite() {
        return;
    }
    let mut draw = world.random_float_for(crate::world::RngStream::Ai, 0.0, total);
    let mut choice = options.last();
    for option in &options {
        draw -= option.0.chance;
        if draw <= 0.0 {
            choice = Some(option);
            break;
        }
    }
    let Some((attack, weapon)) = choice else {
        return;
    };
    let event = attack.event.clone();
    if let Some(state) = world.loaded_actor_mut(actor) {
        state.npc_strike = Some(NpcStrike {
            event: event.clone(),
            target,
            weapon: *weapon,
            hand: attack.hand,
            contact_used: false,
            finished: false,
            ready_at: now,
            recovery: attack.recovery,
        });
    }
    world.request_animation(actor, &event);
}

pub(crate) fn finish(world: &mut World, actor: GlobalFormId) {
    let now = world.clock.real_seconds;
    if let Some(strike) = world
        .loaded_actor_mut(actor)
        .and_then(|state| state.npc_strike.as_mut())
        .filter(|strike| !strike.finished)
    {
        strike.ready_at = now + f64::from(strike.recovery.max(0.25));
        strike.finished = true;
    }
}

pub(super) fn cancel(world: &mut World, actor: GlobalFormId) {
    let active = world
        .actor_ref(actor)
        .and_then(|state| state.npc_strike.as_ref())
        .is_some_and(|strike| !strike.finished);
    if active {
        finish(world, actor);
        world.request_animation(actor, "attackStop");
    }
}

impl World {
    /// Consume the authored hand's sole contact opportunity before a host query.
    pub fn take_npc_strike_contact(
        &mut self,
        actor: GlobalFormId,
        hand: Hand,
    ) -> Result<Option<(GlobalFormId, f32)>, Error> {
        let Some(state) = self.loaded_actor_mut(actor) else {
            return Ok(None);
        };
        if state.dead || state.unconscious || !state.in_combat {
            return Ok(None);
        }
        let Some(strike) = state.npc_strike.as_mut().filter(|strike| {
            !strike.finished
                && !strike.contact_used
                && strike.hand == hand
                && state.combat_target == Some(strike.target)
        }) else {
            return Ok(None);
        };
        strike.contact_used = true;
        let weapon = strike.weapon;
        let event = strike.event.clone();
        if !actor_attacks(self, actor)?
            .iter()
            .any(|attack| attack.event == event)
        {
            return Ok(None);
        }
        if self.hand_weapon(actor, hand) != (weapon != UNARMED).then_some(weapon) {
            return Ok(None);
        }
        Ok(Some((weapon, self.combat_approach_distance(actor)?)))
    }
}

pub(crate) fn write_strike(actor: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(actor.npc_strike.is_some());
    if let Some(strike) = &actor.npc_strike {
        writer.str(&strike.event);
        write_form_id(writer, strike.target);
        write_form_id(writer, strike.weapon);
        writer.bool(strike.hand == Hand::Left);
        writer.bool(strike.contact_used);
        writer.bool(strike.finished);
        writer.f64(strike.ready_at);
        writer.f32(strike.recovery);
    }
}

pub(crate) fn read_strike(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    actor.npc_strike = if reader.bool()? {
        let event = reader.str()?;
        let target = read_form_id(reader)?;
        let weapon = read_form_id(reader)?;
        let hand = if reader.bool()? {
            Hand::Left
        } else {
            Hand::Right
        };
        let contact_used = reader.bool()?;
        let finished = reader.bool()?;
        let ready_at = reader.f64()?;
        let recovery = reader.f32()?;
        if event.is_empty() || !ready_at.is_finite() || !recovery.is_finite() || recovery < 0.0 {
            return Err(VmError::State("invalid NPC strike".into()));
        }
        Some(NpcStrike {
            event,
            target,
            weapon,
            hand,
            contact_used,
            finished,
            ready_at,
            recovery,
        })
    } else {
        None
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin};
    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }
    fn fixture() -> (World, GlobalFormId, GlobalFormId) {
        let mut plugin = Plugin::new(FileHeader::default());
        let mut race = vec![0_u8; 128];
        for (offset, value) in [(16, 1.0_f32), (20, 1.0), (96, 60.0), (100, 512.0)] {
            race[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut attack = vec![0_u8; 44];
        for (offset, value) in [(0, 1.0_f32), (4, 1.0), (20, 65.0), (36, 1.5), (40, 1.0)] {
            attack[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(b"RACE", 0xa00)
                .sub(b"DATA", race)
                .sub(b"ATKD", attack)
                .zsub(b"ATKE", "attackStart_Fixture"),
        );
        plugin.push_top(
            Record::new(b"NPC_", 0x800)
                .sub(b"RNAM", 0xa00_u32.to_le_bytes())
                .sub(b"ATKR", 0xa00_u32.to_le_bytes()),
        );
        plugin.push_top(Record::new(b"ACHR", 0x801).sub(b"NAME", 0x800_u32.to_le_bytes()));
        plugin.push_top(Record::new(b"SNDR", 0xa02).zsub(b"EDID", "FixtureStrikeVocal"));
        let mut order = LoadOrder::new();
        order.add("Strikes.esm", plugin).unwrap();
        let mut world = World::new(order);
        let actor = id(0x801);
        let target = world.player();
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(target);
        (world, actor, target)
    }
    #[test]
    fn npc_strike_queues_an_authored_event_once_until_the_graph_finishes() {
        let (mut world, actor, target) = fixture();
        for _ in 0..60 {
            start(&mut world, actor, target);
        }
        assert_eq!(world.animation_requests.len(), 1);
        assert_eq!(world.animation_requests[0].animation, "attackStart_Fixture");
    }
    #[test]
    fn npc_strike_consumes_only_the_selected_hands_first_contact() {
        let (mut world, actor, target) = fixture();
        start(&mut world, actor, target);
        assert_eq!(
            world.take_npc_strike_contact(actor, Hand::Left).unwrap(),
            None
        );
        assert_eq!(
            world.take_npc_strike_contact(actor, Hand::Right).unwrap(),
            Some((UNARMED, 512.0))
        );
        assert_eq!(
            world.take_npc_strike_contact(actor, Hand::Right).unwrap(),
            None
        );
    }
    #[test]
    fn npc_strike_recovery_starts_when_the_authored_graph_stops() {
        let (mut world, actor, target) = fixture();
        start(&mut world, actor, target);
        world.clock.real_seconds = 2.0;
        finish(&mut world, actor);
        world.clock.real_seconds = 3.49;
        start(&mut world, actor, target);
        assert_eq!(world.animation_requests.len(), 1);
        world.clock.real_seconds = 3.5;
        start(&mut world, actor, target);
        assert_eq!(world.animation_requests.len(), 2);
    }
    #[test]
    fn npc_strike_save_retains_an_already_consumed_contact() {
        let (mut world, actor, target) = fixture();
        start(&mut world, actor, target);
        assert!(world
            .take_npc_strike_contact(actor, Hand::Right)
            .unwrap()
            .is_some());
        let bytes = world.save_state();
        let (mut restored, _, _) = fixture();
        restored.load_state(&bytes).unwrap();
        assert_eq!(
            world.actor_ref(actor).unwrap().npc_strike,
            restored.actor_ref(actor).unwrap().npc_strike
        );
        assert_eq!(
            restored
                .take_npc_strike_contact(actor, Hand::Right)
                .unwrap(),
            None
        );
    }
    #[test]
    fn npc_strike_sound_annotation_resolves_the_authored_descriptor() {
        let (mut world, actor, _) = fixture();
        world.animation_event(actor, "SoundPlay.FixtureStrikeVocal", &mut |_, _| {
            soe_papyrus::ObjectId(1)
        });
        assert_eq!(world.actor_sound_requests.len(), 1);
        assert_eq!(world.actor_sound_requests[0].descriptor, id(0xa02));
    }

    #[test]
    fn npc_strike_changed_target_cannot_consume_a_contact() {
        let (mut world, actor, target) = fixture();
        start(&mut world, actor, target);
        world.actor(actor).unwrap().combat_target = Some(id(0x802));
        assert_eq!(
            world.take_npc_strike_contact(actor, Hand::Right).unwrap(),
            None
        );
    }
}
