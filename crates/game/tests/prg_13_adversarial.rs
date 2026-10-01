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

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION.object)
            .zsub(T(b"EDID"), "JailFaction")
            .sub(T(b"DATA"), 0x40_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"MISC"), 0x702).zsub(T(b"EDID"), "PermittedItem"));
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
    let game = game();
    let mut world = game.world_mut();
    let player = world.player();
    world.actor(player).unwrap();
    // FACT defines STOL, PLCN and JOUT for stolen goods, belongings and jail outfit.
    // Serving and clearing prison must return permitted belongings without duplicating them.
    let permitted_item = GlobalFormId {
        file: 0,
        object: 0x702,
    };
    world.add_item(player, permitted_item, 2);
    let before = world.item_count(player, permitted_item);

    world.faction(FACTION).unwrap().crime_gold_violent = 1;
    world.serve_jail_time(FACTION).unwrap();
    world.clear_prison();

    assert_eq!(world.item_count(player, permitted_item), before);
}
