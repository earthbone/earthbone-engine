// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for PRG-08's resolved form dispatch and malformed data.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const PERK: u32 = 0x800;
const REQUIRED_PERK: u32 = 0x801;
const ENTRY: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
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

fn value_handler(world: &mut World, invocation: &EntryPointInvocation) {
    let value = f32::from_le_bytes(invocation.data.as_slice().try_into().unwrap());
    world
        .set_actor_value(invocation.actor, "health", value)
        .unwrap();
}

fn entry(value: f32) -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), value.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn has_perk_condition(perk: u32) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&448u16.to_le_bytes());
    bytes[12..16].copy_from_slice(&perk.to_le_bytes());
    bytes
}

#[test]
fn prg_08_stored_effect_rank_zero_runs_for_first_owned_rank() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(entry(3.0));
    let mut order = LoadOrder::new();
    order.add("RankZero.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut().actor(actor).unwrap().perks.insert(id(PERK));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, value_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 3.0);
}

#[test]
fn prg_08_false_entry_condition_prevents_handler_execution() {
    let conditioned = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 1])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"CTDA"), has_perk_condition(REQUIRED_PERK))
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(conditioned);
    plugin.push_top(Record::new(Tag(*b"PERK"), REQUIRED_PERK));
    let mut order = LoadOrder::new();
    order.add("FalseCondition.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut().actor(actor).unwrap().perks.insert(id(PERK));
    game.world_mut()
        .set_actor_value(actor, "health", 17.0)
        .unwrap();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, value_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(0)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 17.0);
}

#[test]
fn prg_08_dispatch_reads_the_winning_perk_record() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(entry(1.0));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(entry(2.0));

    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut().actor(actor).unwrap().perks.insert(id(PERK));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, value_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 2.0);
}

#[test]
fn prg_08_missing_required_section_end_marker_is_rejected() {
    let malformed = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 4.0f32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(malformed);
    let mut order = LoadOrder::new();
    order.add("MalformedPerk.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut().actor(actor).unwrap().perks.insert(id(PERK));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, value_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Malformed(id(PERK)))
    );
}

#[test]
fn prg_08_unknown_perk_section_type_is_rejected_explicitly() {
    let malformed = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [3, 0, 0])
        .sub(Tag(*b"PRKF"), []);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(malformed);
    let mut order = LoadOrder::new();
    order.add("UnknownSection.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut().actor(actor).unwrap().perks.insert(id(PERK));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, value_handler);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::UnsupportedSection(id(PERK)))
    );
}

#[test]
fn prg_08_unregistered_entry_point_errors_without_owned_perks() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"PERK"), PERK));
    let mut order = LoadOrder::new();
    order.add("UnregisteredEntry.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    let dispatcher = EntryPointDispatcher::default();

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Unregistered(ENTRY))
    );
}
