// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial record-boundary checks for PRG-17 perk entry decoding.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const PERK: u32 = 0x800;
const ENTRY: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn entry(rank: u8, priority: u8) -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, rank, priority])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 1.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn game(record: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Prg17.esm", plugin).unwrap();
    let game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
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
fn prg_17_raw_rank_and_priority_accept_full_byte_range() {
    let game = game(entry(u8::MAX, u8::MAX));
    let actor = game.world_mut().player();
    let invocations = soe_game::perks::entry_point_invocations(
        &mut game.world_mut(),
        &game.vm,
        actor,
        ENTRY,
        &context(actor),
    )
    .unwrap();

    assert_eq!(invocations.len(), 1);
    assert_eq!(invocations[0].rank, 256);
    assert_eq!(invocations[0].priority, u8::MAX);
}

#[test]
fn prg_17_malformed_later_section_rejects_before_dispatch() {
    let mut record = entry(0, 0);
    record = record.sub(Tag(*b"PRKE"), [2, 1]);
    let mut game = game(record);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, noop);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Malformed(id(PERK)))
    );
}
