// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-39 NPC perk dispatch fixtures, using xEdit PRKR and PRKE layouts.

use crate::common::installed;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{
        entry_point_invocations, DispatchError, EntryPointContext, EntryPointDispatcher,
        EntryPointInvocation,
    },
    Game, World,
};

const BASE: u32 = 0x800;
const PERK: u32 = 0x810;
const ENTRY: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placement(perk: u32, rank: u8) -> Vec<u8> {
    let mut bytes = perk.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[rank, 0xAB, 0xCD, 0xEF]);
    bytes
}

fn fixture(entries: &[Vec<u8>]) -> Game {
    let game = fixture_with_records(entries, vec![perk(PERK), perk(PERK + 1)]);
    for actor in [id(0x900), id(0x901)] {
        game.world_mut()
            .set_actor_value(actor, "health", 10.0)
            .unwrap();
    }
    game
}

fn perk(object: u32) -> Record {
    Record::new(Tag(*b"PERK"), object)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 1])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 1.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
        .sub(Tag(*b"PRKE"), [2, 1, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 1])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 100.0_f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn fixture_with_records(entries: &[Vec<u8>], records: Vec<Record>) -> Game {
    let mut npc = Record::new(Tag(*b"NPC_"), BASE);
    for entry in entries {
        npc = npc.sub(Tag(*b"PRKR"), entry.clone());
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc);
    for actor in [0x900_u32, 0x901] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    }
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Cmb39.esm", plugin).unwrap();
    Game::new(order)
}

fn context(actor: GlobalFormId) -> EntryPointContext {
    EntryPointContext {
        tabs: vec![actor],
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
fn cmb_39_duplicate_prkr_dispatches_each_perk_once() {
    for ranks in [[1, 1], [1, 2], [2, 1]] {
        let mut game = fixture(&[
            placement(PERK, ranks[0]),
            placement(PERK + 1, 1),
            placement(PERK, ranks[1]),
        ]);
        let actor = id(0x900);
        assert_eq!(
            dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
            Ok(2)
        );
        assert_eq!(game.world_mut().actor_value(actor, "health"), 12.0);
    }
}

#[test]
fn cmb_39_duplicate_prkr_logs_the_base_once_across_calls_and_references() {
    // CONTEXT: tracing's callsite cache is shared with concurrent test threads.
    if std::env::var("SOE_CMB_39_LOG_CHILD").as_deref() != Ok("1") {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &crate::common::child_test_name(
                    module_path!(),
                    "cmb_39_duplicate_prkr_logs_the_base_once_across_calls_and_references",
                ),
                "--nocapture",
                "--test-threads=1",
            ])
            .env("SOE_CMB_39_LOG_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let mut game = fixture(&[
        placement(PERK, 1),
        placement(PERK, 2),
        placement(PERK + 1, 1),
        placement(PERK + 1, 1),
    ]);
    let (subscriber, control) = soe_trace::logging::subscriber("warn", None, |_| {}).unwrap();
    soe_trace::tracing::subscriber::with_default(subscriber, || {
        for actor in [id(0x900), id(0x900), id(0x901)] {
            assert_eq!(
                dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
                Ok(2)
            );
        }
    });
    let lines = control.window(0, u64::MAX);
    assert_eq!(
        lines.len(),
        1,
        "one warning per base, not per entry or reference"
    );
    assert_eq!(
        lines[0].count, 1,
        "duplicates must not rely on log collapsing"
    );
    assert_eq!(lines[0].fields["form_id"], id(BASE).to_string());
}

#[test]
fn cmb_39_malformed_prkr_still_rejects_dispatch_without_handler_changes() {
    for length in [4, 7, 9] {
        let mut malformed = placement(PERK, 1);
        malformed.resize(length, 0);
        let mut game = fixture_with_records(
            &[placement(PERK, 1), placement(PERK, 1), malformed],
            vec![perk(PERK)],
        );
        let actor = id(0x900);
        let health = game.world_mut().actor_value(actor, "health");
        assert_eq!(
            dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
            Err(DispatchError::Malformed(id(BASE)))
        );
        assert_eq!(game.world_mut().actor_value(actor, "health"), health);
    }
}

#[test]
fn cmb_39_sparse_condition_tabs_allow_unconditional_and_conditional_entries() {
    for tab in [None, Some(1)] {
        let mut record = Record::new(Tag(*b"PERK"), PERK)
            .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 0, 0])
            .sub(Tag(*b"DATA"), [ENTRY, 2, 3]);
        if let Some(tab) = tab {
            let mut condition = vec![0; 32];
            condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
            condition[8..10].copy_from_slice(&448_u16.to_le_bytes());
            condition[12..16].copy_from_slice(&PERK.to_le_bytes());
            record = record
                .sub(Tag(*b"PRKC"), [tab])
                .sub(Tag(*b"CTDA"), condition);
        }
        record = record
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), 1.0_f32.to_le_bytes())
            .sub(Tag(*b"PRKF"), []);
        let mut game = fixture_with_records(&[placement(PERK, 1)], vec![record]);
        let actor = id(0x900);
        let supplied = EntryPointContext {
            tabs: vec![actor, id(0x901), actor],
            ..context(actor)
        };
        assert_eq!(
            dispatcher().dispatch(&mut game, actor, ENTRY, &supplied),
            Ok(1)
        );
        if tab.is_some() {
            game.world_mut().actor(id(0x901)).unwrap().perks.clear();
            // CONTEXT: the tab subject's canonical ownership controls HasPerk.
            assert_eq!(
                dispatcher().dispatch(&mut game, actor, ENTRY, &supplied),
                Ok(0)
            );
            assert_eq!(
                dispatcher().dispatch(&mut game, actor, ENTRY, &context(actor)),
                Err(DispatchError::MissingTab {
                    perk: id(PERK),
                    tab: 1
                })
            );
        }
    }
}

#[test]
fn cmb_39_installed_player_combat_perks_accept_sparse_tabs() {
    let Some(game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping installed sparse perk tabs check");
        return;
    };
    let actor = game.world_mut().player();
    let supplied = EntryPointContext {
        tabs: vec![actor; 3],
        target: Some(actor),
        ..context(actor)
    };
    let mut world = game.world_mut();
    let alchemy = world.form("AlchemySkillBoosts").unwrap();
    assert_eq!(world.tag_of(alchemy), Some(Tag(*b"PERK")));
    let invocations =
        entry_point_invocations(&mut world, &game.vm, actor, ENTRY, &supplied).unwrap();
    assert!(
        invocations
            .iter()
            .any(|invocation| invocation.perk == alchemy),
        "the installed player's sparse AlchemySkillBoosts sections must dispatch"
    );
}
