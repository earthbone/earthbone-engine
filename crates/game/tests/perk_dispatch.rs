// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const PERK: u32 = 0x800;
const REQUIRED: u32 = 0x801;
const ENTRY: u8 = 35;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn condition(function: u16, parameter: u32, value: f32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[4..8].copy_from_slice(&value.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("PerkDispatch.esm", plugin).unwrap();
    let game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
    game.world_mut()
        .set_actor_value(actor, "health", 10.0)
        .unwrap();
    game
}

fn apply_numeric(world: &mut World, invocation: &EntryPointInvocation) {
    world
        .set_actor_value(
            invocation.actor,
            "health",
            f32::from(invocation.rank * 100 + u16::from(invocation.priority)),
        )
        .unwrap();
}

fn context(actor: GlobalFormId) -> EntryPointContext {
    EntryPointContext {
        tabs: vec![actor],
        target: None,
        quest: None,
        tempering_item: None,
    }
}

#[test]
fn prg_08_dispatches_owned_entries_in_record_order_and_passes_rank() {
    let record = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 2, 7])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
        .sub(Tag(*b"PRKE"), [2, 0, 1])
        .sub(Tag(*b"DATA"), [ENTRY, 3, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 2.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut game = game(vec![record]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, apply_numeric);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(2)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 101.0);
}

#[test]
fn prg_08_entry_conditions_gate_handlers_and_unregistered_ids_error() {
    let record = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 1])
        .sub(Tag(*b"PRKC"), [0])
        .sub(Tag(*b"CTDA"), condition(448, REQUIRED, 1.0))
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 5.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut game = game(vec![record, Record::new(Tag(*b"PERK"), REQUIRED)]);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(REQUIRED));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, apply_numeric);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 100.0);
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY + 1, &context(actor)),
        Err(DispatchError::Unregistered(ENTRY + 1))
    );
}

#[test]
fn prg_08_false_conditions_and_missing_tabs_do_not_run_handlers() {
    let record = Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 2, 1])
        .sub(Tag(*b"PRKC"), [1])
        .sub(Tag(*b"CTDA"), condition(448, REQUIRED, 1.0))
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 5.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), []);
    let mut game = game(vec![record, Record::new(Tag(*b"PERK"), REQUIRED)]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, apply_numeric);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::MissingTab {
            perk: id(PERK),
            tab: 1
        })
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 10.0);
    assert_eq!(
        dispatcher.dispatch(
            &mut game,
            actor,
            ENTRY,
            &EntryPointContext {
                tabs: vec![actor, actor],
                target: None,
                quest: None,
                tempering_item: None,
            }
        ),
        Ok(0)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 10.0);
}
