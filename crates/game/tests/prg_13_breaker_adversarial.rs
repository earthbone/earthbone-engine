// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Source-based edge cases for FACT jail property transfers.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};
const PLCN: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x710,
};
const STOL: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x711,
};
const ITEM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x702,
};
const OWNER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x703,
};

fn game(plcn: Option<Vec<u8>>, stol: Option<Vec<u8>>, include_evidence: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let mut faction = Record::new(T(b"FACT"), FACTION.object)
        .zsub(T(b"EDID"), "JailFaction")
        .sub(T(b"DATA"), 0x40_u32.to_le_bytes());
    if let Some(bytes) = plcn {
        faction = faction.sub(T(b"PLCN"), bytes);
    }
    if let Some(bytes) = stol {
        faction = faction.sub(T(b"STOL"), bytes);
    }
    plugin.push_top(faction);
    plugin.push_top(Record::new(T(b"MISC"), ITEM.object).zsub(T(b"EDID"), "Item"));
    plugin.push_top(Record::new(T(b"NPC_"), OWNER.object).zsub(T(b"EDID"), "Owner"));
    plugin.push_top(Record::new(T(b"CONT"), 0x704).zsub(T(b"EDID"), "JailChest"));
    plugin.push_top(
        Record::new(T(b"REFR"), PLCN.object).sub(T(b"NAME"), 0x704_u32.to_le_bytes()),
    );
    if include_evidence {
        plugin.push_top(
            Record::new(T(b"REFR"), STOL.object).sub(T(b"NAME"), 0x704_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn form_bytes(form: GlobalFormId) -> Vec<u8> {
    form.object.to_le_bytes().to_vec()
}

fn owner_stack() -> Stack {
    Stack::new(
        1,
        vec![Extra {
            kind: 33,
            fields: vec![Field::Form(Some(OWNER))],
        }],
    )
    .unwrap()
}

#[test]
fn prg_13_missing_stol_keeps_stolen_stack_with_player_through_release() {
    let game = game(Some(form_bytes(PLCN)), None, false);
    let mut world = game.world_mut();
    let player = world.player();
    world.add_item(player, ITEM, 2);
    world
        .reference(player)
        .unwrap()
        .inventory
        .restore(ITEM, 3, vec![owner_stack()])
        .unwrap();
    world.actor(player).unwrap().crime_faction = Some(FACTION);
    world.faction(FACTION).unwrap().crime_gold_violent = 1;

    world.serve_jail_time(FACTION).unwrap();
    assert_eq!(world.item_count(player, ITEM), 1);
    assert_eq!(world.item_count(PLCN, ITEM), 2);
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 3);
    assert_eq!(world.item_count(PLCN, ITEM), 0);
    assert_eq!(world.item_count(STOL, ITEM), 0);
}

#[test]
fn prg_13_malformed_plcn_field_leaves_inventory_with_player() {
    let game = game(Some(vec![0x10, 0x00]), Some(form_bytes(STOL)), true);
    let mut world = game.world_mut();
    let player = world.player();
    world.add_item(player, ITEM, 2);
    world.actor(player).unwrap().crime_faction = Some(FACTION);
    world.faction(FACTION).unwrap().crime_gold_violent = 100;

    assert_eq!(world.serve_jail_time(FACTION), Ok(1));
    assert_eq!(world.item_count(player, ITEM), 2);
    assert_eq!(world.item_count(PLCN, ITEM), 0);
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 2);
}

#[test]
fn prg_13_plcn_must_resolve_to_a_reference_record() {
    let game = game(Some(form_bytes(ITEM)), Some(form_bytes(STOL)), true);
    let mut world = game.world_mut();
    let player = world.player();
    world.add_item(player, ITEM, 2);
    world.actor(player).unwrap().crime_faction = Some(FACTION);
    world.faction(FACTION).unwrap().crime_gold_violent = 1;

    world.serve_jail_time(FACTION).unwrap();
    assert_eq!(world.item_count(player, ITEM), 2);
    assert_eq!(world.item_count(ITEM, ITEM), 0);
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 2);
}

#[test]
fn prg_13_confiscated_property_and_release_survive_engine_save_load() {
    let initial_game = game(Some(form_bytes(PLCN)), Some(form_bytes(STOL)), true);
    let mut world = initial_game.world_mut();
    let player = world.player();
    world.add_item(player, ITEM, 2);
    world
        .reference(player)
        .unwrap()
        .inventory
        .restore(ITEM, 3, vec![owner_stack()])
        .unwrap();
    world.actor(player).unwrap().crime_faction = Some(FACTION);
    world.faction(FACTION).unwrap().crime_gold_violent = 1;
    world.serve_jail_time(FACTION).unwrap();
    assert_eq!(world.item_count(player, ITEM), 0);

    let save = world.save_state();
    drop(world);
    let restored = game(Some(form_bytes(PLCN)), Some(form_bytes(STOL)), true);
    let mut world = restored.world_mut();
    world.load_state(&save).unwrap();
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 2);
    assert_eq!(world.item_count(PLCN, ITEM), 0);
    assert_eq!(world.item_count(STOL, ITEM), 1);
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 2);
    assert_eq!(world.item_count(STOL, ITEM), 1);
}
