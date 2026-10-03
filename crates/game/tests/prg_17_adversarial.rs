// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for the sourced PRG-17 record fields and state boundary.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};
use std::collections::BTreeSet;

const PERK: u32 = 0x800;
const REQUIRED: u32 = 0x801;
const ENTRY: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameter: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn entry(perk: u32, rank: u8, priority: u8, tab: Option<u8>) -> Record {
    let mut record = Record::new(Tag(*b"PERK"), perk)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, rank, priority])
        .sub(Tag(*b"DATA"), [ENTRY, 1, u8::from(tab.is_some())]);
    if let Some(tab) = tab {
        record = record
            .sub(Tag(*b"PRKC"), [tab])
            .sub(Tag(*b"CTDA"), condition(448, REQUIRED));
    }
    record
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 1.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Prg17.esm", plugin).unwrap();
    let game = Game::new(order);
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .perks
        .insert(id(PERK));
    game.world_mut()
        .actor(player)
        .unwrap()
        .perks
        .insert(id(REQUIRED));
    game
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

fn noop(_: &mut World, _: &EntryPointInvocation) {}

#[test]
fn prg_17_prke_raw_rank_zero_and_one_are_one_based_and_priority_is_retained() {
    const PERK_TWO: u32 = 0x802;
    let mut game = game(vec![
        entry(PERK, 0, 17, None).sub(Tag(*b"NNAM"), PERK_TWO.to_le_bytes()),
        entry(PERK_TWO, 1, 4, None),
    ]);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK_TWO));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, noop);

    let observed = soe_game::perks::entry_point_invocations(
        &mut game.world_mut(),
        &game.vm,
        actor,
        ENTRY,
        &context(actor),
    )
    .unwrap()
    .iter()
    .map(|invocation| (invocation.rank, invocation.priority))
    .collect::<BTreeSet<_>>();
    assert_eq!(observed, BTreeSet::from([(1, 17), (2, 4)]));
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(2)
    );
}

#[test]
fn prg_17_condition_tab_uses_its_supplied_actor_and_missing_tab_errors() {
    let mut game = game(vec![
        entry(PERK, 0, 0, Some(0)),
        Record::new(Tag(*b"PERK"), REQUIRED),
    ]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, noop);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    let empty = EntryPointContext {
        tabs: vec![],
        ..context(actor)
    };
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &empty),
        Err(DispatchError::MissingTab {
            perk: id(PERK),
            tab: 0
        })
    );
}

#[test]
fn prg_17_owned_perk_identity_survives_game_save_load_before_dispatch() {
    let original = game(vec![entry(PERK, 0, 0, None)]);
    let actor = original.world_mut().player();
    let bytes = original.save_state().unwrap();
    let mut restored = game(vec![entry(PERK, 0, 0, None)]);
    restored.load_state(&bytes).unwrap();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, noop);

    assert_eq!(
        dispatcher.dispatch(&mut restored, actor, ENTRY, &context(actor)),
        Ok(1)
    );
}
