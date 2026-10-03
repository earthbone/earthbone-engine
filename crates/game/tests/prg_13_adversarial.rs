// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial PRG-13 jail sentence and property cases.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};
const PERMITTED_CHEST: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x710,
};
const EVIDENCE_CHEST: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x711,
};
const ITEM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x702,
};

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION.object)
            .zsub(T(b"EDID"), "JailFaction")
            .sub(T(b"DATA"), 0x40_u32.to_le_bytes())
            .sub(T(b"PLCN"), PERMITTED_CHEST.object.to_le_bytes())
            .sub(T(b"STOL"), EVIDENCE_CHEST.object.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"MISC"), 0x702).zsub(T(b"EDID"), "PermittedItem"));
    plugin.push_top(Record::new(T(b"NPC_"), 0x703).zsub(T(b"EDID"), "ItemOwner"));
    plugin.push_top(Record::new(T(b"CONT"), 0x704).zsub(T(b"EDID"), "JailChest"));
    plugin.push_top(
        Record::new(T(b"REFR"), PERMITTED_CHEST.object).sub(T(b"NAME"), 0x704_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), EVIDENCE_CHEST.object).sub(T(b"NAME"), 0x704_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

#[test]
fn prg_13_empty_repeat_serve_does_not_add_days_or_advance_clock() {
    let game = game();
    let mut world = game.world_mut();
    let player = world.player();
    world.actor(player).unwrap();
    world.faction(FACTION).unwrap().crime_gold_violent = 100;
    assert_eq!(world.serve_jail_time(FACTION), Ok(1));
    let days = world.actor(player).unwrap().days_in_jail;
    let clock = world.clock.game_days;

    assert_eq!(world.serve_jail_time(FACTION), Ok(0));
    assert_eq!(world.actor(player).unwrap().days_in_jail, days);
    assert_eq!(world.clock.game_days, clock);
}

#[test]
fn prg_13_untracked_faction_rejects_without_changing_player_or_bounty() {
    let game = game();
    let mut world = game.world_mut();
    let player = world.player();
    world.actor(player).unwrap().days_in_jail = 2;
    let before = world.clock.game_days;
    world.faction(FACTION).unwrap().tracks_crime = false;
    world.faction(FACTION).unwrap().crime_gold_nonviolent = 150;

    assert!(world.serve_jail_time(FACTION).is_err());
    assert_eq!(world.clock.game_days, before);
    assert_eq!(world.actor(player).unwrap().days_in_jail, 2);
    assert_eq!(world.faction(FACTION).unwrap().crime_gold_nonviolent, 150);
}

#[test]
fn prg_13_release_restores_permitted_property_and_preserves_unrelated_inventory() {
    let initial_game = game();
    let mut world = initial_game.world_mut();
    let player = world.player();
    world.actor(player).unwrap().crime_faction = Some(FACTION);
    world.add_item(player, ITEM, 2);
    world
        .reference(player)
        .unwrap()
        .inventory
        .restore(
            ITEM,
            3,
            vec![soe_game::inventory::Stack::new(
                1,
                vec![soe_game::inventory::Extra {
                    kind: 33,
                    fields: vec![soe_game::inventory::Field::Form(Some(GlobalFormId {
                        file: 0,
                        object: 0x703,
                    }))],
                }],
            )
            .unwrap()],
        )
        .unwrap();
    assert_eq!(world.item_count(player, ITEM), 3);

    world.faction(FACTION).unwrap().crime_gold_violent = 1;
    world.serve_jail_time(FACTION).unwrap();
    assert_eq!(world.item_count(player, ITEM), 0);
    assert_eq!(world.item_count(PERMITTED_CHEST, ITEM), 2);
    assert_eq!(world.item_count(EVIDENCE_CHEST, ITEM), 1);

    world.add_item(player, ITEM, 1);
    let save = world.save_state();
    drop(world);
    let restored = game();
    let mut world = restored.world_mut();
    world.load_state(&save).unwrap();
    world.clear_prison();

    assert_eq!(world.item_count(player, ITEM), 3);
    assert_eq!(world.item_count(PERMITTED_CHEST, ITEM), 0);
    assert_eq!(world.item_count(EVIDENCE_CHEST, ITEM), 1);
    world.clear_prison();
    assert_eq!(world.item_count(player, ITEM), 3);
}
