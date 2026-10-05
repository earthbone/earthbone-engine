// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for protected-kill dispatch with NPC base perks.

use crate::common::installed;
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

fn prkr(perk: u32, rank: u8, tail: [u8; 3]) -> Vec<u8> {
    let mut bytes = perk.to_le_bytes().to_vec();
    bytes.push(rank);
    bytes.extend_from_slice(&tail);
    bytes
}

fn perk() -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 3])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 2.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn fixture(entries: &[Vec<u8>]) -> Game {
    let mut flags_bytes = vec![0; 24];
    flags_bytes[..4].copy_from_slice(&flags::PROTECTED.to_le_bytes());
    let mut npc = Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"ACBS"), flags_bytes);
    for entry in entries {
        npc = npc.sub(Tag(*b"PRKR"), entry.clone());
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc);
    plugin.push_top(perk());
    for actor in [0x900_u32, 0x901] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    }
    let mut order = LoadOrder::new();
    order.add("Cmb39Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn context(actor: GlobalFormId) -> EntryPointContext {
    EntryPointContext {
        tabs: vec![actor; 3],
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: None,
    }
}

fn add_health(world: &mut World, invocation: &EntryPointInvocation) {
    let value = f32::from_le_bytes(invocation.data.as_slice().try_into().unwrap());
    let health = world.actor_value(invocation.actor, "health");
    world
        .set_actor_value(invocation.actor, "health", health + value)
        .unwrap();
}

fn dispatcher() -> EntryPointDispatcher {
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, add_health);
    dispatcher
}

#[test]
fn cmb_39_duplicate_ranks_are_ignored_and_effect_runs_once() {
    for ranks in [[0, 0], [1, 255], [255, 1]] {
        let mut game = fixture(&[
            prkr(PERK, ranks[0], [0, 0, 0]),
            prkr(PERK, ranks[1], [0xAB, 0xCD, 0xEF]),
        ]);
        let actor = id(0x900);
        assert_eq!(
            dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
            Ok(1),
            "the unused PRKR rank byte must not change ownership or multiply effects"
        );
        assert_eq!(game.world_mut().actor_value(actor, "health"), 2.0);
    }
}

#[test]
fn cmb_39_malformed_prkr_after_duplicate_rejects_before_handler() {
    let mut malformed = prkr(PERK, 1, [0, 0, 0]);
    malformed.pop();
    let mut game = fixture(&[
        prkr(PERK, 1, [0, 0, 0]),
        prkr(PERK, 2, [0, 0, 0]),
        malformed,
    ]);
    let actor = id(0x900);
    assert_eq!(
        dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Malformed(id(BASE)))
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 0.0);
}

#[test]
fn cmb_39_duplicate_warning_is_once_for_shared_base_across_repeated_calls() {
    let mut game = fixture(&[prkr(PERK, 1, [0, 0, 0]), prkr(PERK, 2, [0, 0, 0])]);
    let (subscriber, control) = soe_trace::logging::subscriber("warn", None, |_| {}).unwrap();
    soe_trace::tracing::subscriber::with_default(subscriber, || {
        for actor in [id(0x900), id(0x900), id(0x901)] {
            assert_eq!(
                dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
                Ok(1)
            );
        }
    });
    let lines = control.window(0, u64::MAX);
    assert_eq!(lines.len(), 1, "one warning per duplicated NPC base");
    assert_eq!(
        lines[0].count, 1,
        "warning is emitted once, not log-collapsed"
    );
    assert_eq!(lines[0].fields["form_id"], id(BASE).to_string());
}

#[test]
fn cmb_39_sparse_unconditional_npc_perk_dispatches_once() {
    let game = fixture(&[prkr(PERK, 1, [0, 0, 0]), prkr(PERK, 2, [0, 0, 0])]);
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
fn cmb_39_installed_protected_npc_player_kill_succeeds_with_authored_perks() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping installed protected NPC check");
        return;
    };
    let mut order = installed::owned_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let found = order
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .find_map(|(form, record)| {
            let npc = soe_game::npc::Npc::from_record(record, false).ok()?;
            (npc.config.flags & flags::PROTECTED != 0
                && npc.config.flags & flags::ESSENTIAL == 0
                && npc.editor_id.as_ref().is_some_and(|name| !name.is_empty()))
            .then_some(form)
        })
        .expect("installed load order has a named protected non-essential NPC");
    let mut patch = Plugin::new(FileHeader {
        masters: order.files().iter().map(|file| file.name.clone()).collect(),
        ..FileHeader::default()
    });
    let prefix = (patch.header.masters.len() as u32) << 24;
    let raw_base = (u32::from(found.file) << 24) | found.object;
    patch.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), raw_base.to_le_bytes()));
    let mut data = vec![0; 10];
    data[4..8].copy_from_slice(&8_f32.to_bits().to_le_bytes());
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_bits().to_le_bytes());
    dnam[8..12].copy_from_slice(&1.1_f32.to_bits().to_le_bytes());
    dnam[76..80].copy_from_slice(&6_u32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    patch.push_top(
        Record::new(Tag(*b"WEAP"), 0xA00)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam),
    );
    for (i, (name, value)) in [
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
        patch.push_top(
            Record::new(Tag(*b"GMST"), 0xB00 + i as u32)
                .sub(Tag(*b"EDID"), name.as_bytes().to_vec())
                .sub(Tag(*b"DATA"), value.to_le_bytes().to_vec()),
        );
    }
    for record in &mut patch.records {
        record.form_id |= prefix;
    }
    let file = order.add("Cmb39ProtectedSurvey.esp", patch).unwrap();
    let target = GlobalFormId {
        file: file as u16,
        object: 0x900,
    };
    let weapon = GlobalFormId {
        file: file as u16,
        object: 0xA00,
    };
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "onehanded", 100.).unwrap();
        world.actor(player).unwrap().perks.clear();
        world.add_item(player, weapon, 1);
        world.actor(player).unwrap().equipped.insert(weapon);
        world.set_actor_value(target, "health", 15.).unwrap();
    }
    let result = game
        .melee_hit(MeleeHit {
            attacker: player,
            target,
            weapon,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    assert!(
        result.killed,
        "duplicate NPC PRKR data must not block the protected player kill"
    );
}
