// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial checks for PRG-08 condition-tab errors and handler effects.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};
use std::sync::atomic::{AtomicUsize, Ordering};

const PERK: u32 = 0x800;
const ENTRY: u8 = 35;
static MISSING_TAB_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static MALFORMED_HANDLER_CALLS: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn entry(point: u8) -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [point, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn context(tabs: Vec<GlobalFormId>) -> EntryPointContext {
    EntryPointContext {
        tabs,
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: None,
    }
}

fn missing_tab_handler(_: &mut World, _: &EntryPointInvocation) {
    MISSING_TAB_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
}

fn malformed_handler(_: &mut World, _: &EntryPointInvocation) {
    MALFORMED_HANDLER_CALLS.fetch_add(1, Ordering::SeqCst);
}

fn game_with(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("PerkDispatch.esm", plugin).expect("valid plugin");
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .expect("player exists")
        .perks
        .insert(id(PERK));
    game
}

#[test]
fn prg_08_missing_condition_tab_returns_error_before_handler_runs() {
    MISSING_TAB_HANDLER_CALLS.store(0, Ordering::SeqCst);
    let conditioned = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 1])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"CTDA"), {
            let mut bytes = [0; 32];
            bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
            bytes[8..10].copy_from_slice(&448u16.to_le_bytes());
            bytes[12..16].copy_from_slice(&0x801u32.to_le_bytes());
            bytes
        })
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut game = game_with(vec![conditioned, Record::new(Tag(*b"PERK"), 0x801)]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, missing_tab_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(vec![actor])),
        Err(DispatchError::MissingTab {
            perk: id(PERK),
            tab: 1
        })
    );
    assert_eq!(MISSING_TAB_HANDLER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn prg_08_malformed_later_perk_prevents_earlier_handler_side_effect() {
    MALFORMED_HANDLER_CALLS.store(0, Ordering::SeqCst);
    let valid = entry(ENTRY);
    let malformed = Record::new(Tag(*b"PERK"), 0x801)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0]);
    let mut game = game_with(vec![valid, malformed]);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .expect("player exists")
        .perks
        .insert(id(0x801));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, malformed_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(vec![actor])),
        Err(DispatchError::Malformed(id(0x801)))
    );
    assert_eq!(MALFORMED_HANDLER_CALLS.load(Ordering::SeqCst), 0);
}

#[test]
fn prg_08_unregistered_entry_point_is_rejected_before_record_dispatch() {
    let mut game = game_with(vec![entry(ENTRY)]);
    let actor = game.world_mut().player();
    let dispatcher = EntryPointDispatcher::default();

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(vec![actor])),
        Err(DispatchError::Unregistered(ENTRY))
    );
}
