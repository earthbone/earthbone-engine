// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for NPC PRKR loading and perk entry tabs.

#[allow(dead_code)]
#[path = "common/mod.rs"]
mod common;

use common::installed;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::MeleeHit,
    npc::flags,
    perks::{
        entry_point_invocations, DispatchError, EntryPointContext, EntryPointDispatcher,
        EntryPointInvocation,
    },
    Game, World,
};

const BASE: u32 = 0x800;
const PERK: u32 = 0x810;
const ENTRY: u8 = 36;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn prkr(perk: u32, rank: u8) -> Vec<u8> {
    let mut bytes = perk.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[rank, 0, 0, 0]);
    bytes
}

fn perk(object: u32, next: Option<u32>) -> Record {
    let mut record = Record::new(Tag(*b"PERK"), object)
        .sub(Tag(*b"DATA"), [0, 0, 2, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 3])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 1.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
        .sub(Tag(*b"PRKE"), [2, 1, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 3])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 10.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    if let Some(next) = next {
        record = record.sub(Tag(*b"NNAM"), next.to_le_bytes());
    }
    record
}

fn fixture(prkrs: &[Vec<u8>], malformed: bool) -> Game {
    let mut flags_bytes = vec![0; 24];
    flags_bytes[..4].copy_from_slice(&flags::PROTECTED.to_le_bytes());
    let mut npc = Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), flags_bytes);
    for entry in prkrs {
        npc = npc.sub(Tag(*b"PRKR"), entry.clone());
    }
    if malformed {
        npc = npc.sub(Tag(*b"PRKR"), [0x10, 0x08, 0, 0]);
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc);
    plugin.push_top(perk(PERK, Some(PERK + 1)));
    plugin.push_top(perk(PERK + 1, None));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Cmb39Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .sub(Tag(*b"EDID"), name.as_bytes().to_vec())
                .sub(Tag(*b"DATA"), value.to_le_bytes().to_vec()),
        );
    }
}

fn context(actor: GlobalFormId) -> EntryPointContext {
    EntryPointContext {
        tabs: vec![actor; 3],
        target: Some(actor),
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: None,
    }
}

fn mark_handler(world: &mut World, invocation: &EntryPointInvocation) {
    let previous = world.actor_value(invocation.actor, "health");
    world
        .set_actor_value(invocation.actor, "health", previous + 1.0)
        .unwrap();
}

#[test]
fn cmb_39_repeated_prkr_with_conflicting_unused_ranks_dispatches_once() {
    let game = fixture(&[prkr(PERK, 0), prkr(PERK, u8::MAX)], false);
    let actor = id(0x900);
    let invocations = entry_point_invocations(
        &mut game.world_mut(),
        &game.vm,
        actor,
        ENTRY,
        &context(actor),
    )
    .unwrap();
    assert_eq!(invocations.len(), 1, "one owned identity dispatches once");
    assert_eq!(invocations[0].perk, id(PERK));
    assert_eq!(
        invocations[0].rank, 1,
        "PRKE rank is reported, not PRKR rank"
    );
}

#[test]
fn cmb_39_missing_perk_record_does_not_duplicate_valid_dispatch() {
    let missing = 0x888_u32;
    let game = fixture(&[prkr(missing, 1), prkr(PERK, 1), prkr(PERK, 2)], false);
    let actor = id(0x900);
    let invocations = entry_point_invocations(
        &mut game.world_mut(),
        &game.vm,
        actor,
        ENTRY,
        &context(actor),
    )
    .unwrap();
    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].perk, id(PERK));
}

#[test]
fn cmb_39_malformed_prkr_after_valid_duplicate_fails_before_any_combat_change() {
    let mut game = fixture(&[prkr(PERK, 1), prkr(PERK, 2)], true);
    let actor = id(0x900);
    game.world_mut()
        .set_actor_value(actor, "health", 10.0)
        .unwrap();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, mark_handler);
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Malformed(id(BASE)))
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 10.0);
}

#[test]
fn cmb_39_installed_protected_player_kill_loads_duplicate_player_perks() {
    let Some(mut order) = installed::owned_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping installed protected player kill");
        return;
    };
    let mut found = None;
    for (form, record) in order
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
    {
        let Ok(npc) = soe_game::npc::Npc::from_record(record, false) else {
            continue;
        };
        let protected_only =
            npc.config.flags & flags::PROTECTED != 0 && npc.config.flags & flags::ESSENTIAL == 0;
        if protected_only {
            let name = npc.editor_id.clone().unwrap_or_default();
            if !name.is_empty() {
                found = Some((form, name));
                break;
            }
        }
    }
    let (base, name) = found.expect("installed data has a named protected non-essential NPC");
    let mut patch = Plugin::new(FileHeader {
        masters: order.files().iter().map(|file| file.name.clone()).collect(),
        ..FileHeader::default()
    });
    let own_prefix = (patch.header.masters.len() as u32) << 24;
    let raw_base = (u32::from(base.file) << 24) | base.object;
    patch.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), raw_base.to_le_bytes()));
    patch.push_top(weapon(0xA00, 10, 6));
    settings(&mut patch);
    for record in &mut patch.records {
        record.form_id |= own_prefix;
    }
    let file = order.add("Cmb39ProtectedSurvey.esp", patch).unwrap();
    let target = GlobalFormId {
        file: file as u16,
        object: 0x900,
    };
    let survey_weapon = GlobalFormId {
        file: file as u16,
        object: 0xA00,
    };
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 100.).unwrap();
        world.actor(player).unwrap().perks.clear();
        world.add_item(player, survey_weapon, 1);
        world.actor(player).unwrap().equipped.insert(survey_weapon);
        world.set_actor_value(target, "health", 15.).unwrap();
    }
    let result = game
        .melee_hit(MeleeHit {
            attacker: player,
            target,
            weapon: survey_weapon,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    assert!(
        result.killed,
        "duplicate player PRKR must not block killing installed NPC {name}"
    );
}
