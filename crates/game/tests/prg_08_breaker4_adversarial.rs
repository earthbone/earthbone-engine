// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! PRG-08 explicit unregistered-entry behavior.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher},
    Game,
};

const PERK: u32 = 0x800;
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

fn entry() -> Record {
    Record::new(Tag(*b"PERK"), PERK)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"PRKE"), [2, 0, 0])
        .sub(Tag(*b"DATA"), [ENTRY, 1, 0])
        .sub(Tag(*b"EPFT"), [1])
        .sub(Tag(*b"EPFD"), 20.0f32.to_le_bytes())
        .sub(Tag(*b"PRKF"), [])
}

#[test]
fn prg_08_unregistered_entry_is_explicit_and_does_not_mutate_actor() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(entry());
    let mut order = LoadOrder::new();
    order.add("UnregisteredEntry.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.world_mut().player();
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
    let before = game.world_mut().actor_value(actor, "health");

    let error = EntryPointDispatcher::default()
        .dispatch(&mut game, actor, ENTRY, &context(actor))
        .unwrap_err();

    assert_eq!(error, DispatchError::Unregistered(ENTRY));
    assert_eq!(game.world_mut().actor_value(actor, "health"), before);
}
