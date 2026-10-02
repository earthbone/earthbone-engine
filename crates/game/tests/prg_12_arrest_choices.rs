// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-12 payment, jail and resistance choices for a tracked arrest.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::crime::{ArrestChoice, ArrestOptions, ArrestResult, Error};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};
const GOLD: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x702,
};

fn arrest_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION.object)
            .zsub(T(b"EDID"), "ArrestFaction")
            .sub(T(b"DATA"), 0x40_u32.to_le_bytes())
            .sub(T(b"CRVA"), crime_values()),
    );
    plugin.push_top(Record::new(T(b"MISC"), GOLD.object).zsub(T(b"EDID"), "Gold001"));
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap().crime_faction = Some(FACTION);
    game
}

fn crime_values() -> [u8; 20] {
    let mut values = [0; 20];
    values[0] = 1;
    values[2..4].copy_from_slice(&250_u16.to_le_bytes());
    values
}

fn set_bounty(game: &mut Game, violent: i32, nonviolent: i32) {
    let mut world = game.world_mut();
    let faction = world.faction(FACTION).unwrap();
    faction.crime_gold_violent = violent;
    faction.crime_gold_nonviolent = nonviolent;
    faction.infamy_violent = violent + 100;
    faction.infamy_nonviolent = nonviolent + 200;
}

#[test]
fn prg_12_arrest_offers_pay_only_when_affordable_and_keeps_other_choices() {
    let mut game = arrest_game();
    set_bounty(&mut game, 250, 50);
    let options = game.world_mut().arrest_options().unwrap();
    assert_eq!(
        options,
        ArrestOptions {
            bounty: 300,
            can_pay: false,
            can_serve_time: true,
            can_resist: true,
        },
        "an underfunded player gets jail and resistance but no payment option"
    );
    let gold = game.form("Gold001").unwrap();
    let player = game.world_mut().player();
    game.world_mut().add_item(player, gold, 300);
    assert!(
        game.world_mut().arrest_options().unwrap().can_pay,
        "payment is offered once the purse covers the full bounty"
    );
}

#[test]
fn prg_12_arrest_payment_clears_only_bounty_and_deducts_gold() {
    let mut game = arrest_game();
    set_bounty(&mut game, 250, 50);
    let player = game.world_mut().player();
    game.world_mut().add_item(player, GOLD, 400);

    assert_eq!(
        game.world_mut().resolve_arrest(ArrestChoice::Pay),
        Ok(ArrestResult::Paid { amount: 300 }),
        "payment resolves at the combined bounty amount"
    );
    assert_eq!(game.world_mut().item_count(player, GOLD), 100);
    {
        let mut world = game.world_mut();
        let faction = world.faction(FACTION).unwrap();
        assert_eq!(faction.crime_gold_violent, 0);
        assert_eq!(faction.crime_gold_nonviolent, 0);
        assert_eq!(faction.infamy_violent, 350);
        assert_eq!(faction.infamy_nonviolent, 250);
    }
    assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail, 0);
}

#[test]
fn prg_12_arrest_jail_serves_time_without_charging_purse() {
    let mut game = arrest_game();
    set_bounty(&mut game, 250, 50);
    let player = game.world_mut().player();
    game.world_mut().add_item(player, GOLD, 400);

    assert_eq!(
        game.world_mut().resolve_arrest(ArrestChoice::Jail),
        Ok(ArrestResult::Jailed { days: 3 }),
        "jail uses the existing rounded sentence calculation"
    );
    assert_eq!(game.world_mut().item_count(player, GOLD), 400);
    assert_eq!(game.world_mut().clock.game_days, 3.0);
    assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail, 3);
    assert!(!game.world_mut().actor(player).unwrap().arrested);
    let mut world = game.world_mut();
    let faction = world.faction(FACTION).unwrap();
    assert_eq!(faction.crime_gold_violent, 0);
    assert_eq!(faction.crime_gold_nonviolent, 0);
    assert_eq!(faction.infamy_violent, 350);
    assert_eq!(faction.infamy_nonviolent, 250);
}

#[test]
fn prg_12_arrest_resistance_preserves_all_arrest_accounting() {
    let mut game = arrest_game();
    set_bounty(&mut game, 250, 50);
    let player = game.world_mut().player();
    game.world_mut().add_item(player, GOLD, 400);

    assert_eq!(
        game.world_mut().resolve_arrest(ArrestChoice::Resist),
        Ok(ArrestResult::Resisted),
        "resistance leaves the player free of a committed arrest outcome"
    );
    assert_eq!(game.world_mut().item_count(player, GOLD), 400);
    assert_eq!(game.world_mut().clock.game_days, 0.0);
    assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail, 0);
    let mut world = game.world_mut();
    let faction = world.faction(FACTION).unwrap();
    assert_eq!(faction.crime_gold_violent, 250);
    assert_eq!(faction.crime_gold_nonviolent, 50);
    assert_eq!(faction.infamy_violent, 350);
    assert_eq!(faction.infamy_nonviolent, 250);
}

#[test]
fn prg_12_arrest_rejected_payment_leaves_the_selected_outcome_uncommitted() {
    let mut game = arrest_game();
    set_bounty(&mut game, 250, 50);
    let player = game.world_mut().player();
    game.world_mut().add_item(player, GOLD, 299);

    assert_eq!(
        game.world_mut().resolve_arrest(ArrestChoice::Pay),
        Err(Error::CannotPay),
        "payment below the exact bounty is rejected"
    );
    assert_eq!(game.world_mut().item_count(player, GOLD), 299);
    {
        let mut world = game.world_mut();
        let faction = world.faction(FACTION).unwrap();
        assert_eq!(faction.crime_gold_violent, 250);
        assert_eq!(faction.crime_gold_nonviolent, 50);
        assert_eq!(faction.infamy_violent, 350);
        assert_eq!(faction.infamy_nonviolent, 250);
    }
    assert_eq!(game.world_mut().clock.game_days, 0.0);
}
