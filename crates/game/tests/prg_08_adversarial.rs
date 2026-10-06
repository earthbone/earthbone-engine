// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const PERK: u32 = 0x800;
const ENTRY: u8 = 35;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn game_with(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("PerkAdversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
    game
}

fn record(rank: u8) -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, rank, 255])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 3.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

fn record_rank(world: &mut World, invocation: &EntryPointInvocation) {
    world
        .set_actor_value(invocation.actor, "health", f32::from(invocation.rank))
        .unwrap();
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

#[test]
fn prg_08_maximum_authored_rank_is_above_the_owned_rank() {
    let mut game = game_with(vec![record(u8::MAX)]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, record_rank);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(0)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 100.0);
}

#[test]
fn prg_08_repeated_dispatch_calls_repeat_the_registered_entry() {
    let mut game = game_with(vec![record(0)]);
    let actor = game.world_mut().player();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, record_rank);

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 1.0);
}

#[test]
fn prg_08_present_unregistered_entry_returns_typed_error() {
    let mut game = game_with(vec![record(0)]);
    let actor = game.world_mut().player();
    let dispatcher = EntryPointDispatcher::default();

    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context(actor)),
        Err(DispatchError::Unregistered(ENTRY))
    );
}
