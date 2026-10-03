// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::natives_2::{call, errors, game, obj};
use soe_papyrus::Value;

/// Faction.psc says SendAssaultAlarm makes a nearby member behave as if
/// assaulted. Recording only witnessed-crime knowledge does not raise alarm.
#[test]
fn scr_25_assault_alarm_alerts_the_nearest_faction_member() {
    let mut game = game();
    let guards = obj(&mut game, "GuardsFaction");
    let witness = game.form("GuardRef").expect("fixture guard");
    let player = game.world_mut().player();

    call(&mut game, &guards, "SendAssaultAlarm", &[]);

    assert!(
        game.world_mut().actor_is_alarmed(witness),
        "the nearby NPC must behave as if the player assaulted them"
    );
    assert!(game.world_mut().actor(witness).unwrap().witnessed_crimes.contains(&player));
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

/// Faction.psc says SendPlayerToJail removes inventory when requested. The
/// current binding logs the option but leaves the player's inventory intact.
#[test]
fn scr_25_jail_inventory_option_removes_player_items() {
    let mut game = game();
    let guards = obj(&mut game, "GuardsFaction");
    let gold = game.form("Gold001").expect("fixture gold");
    let faction = game.form("GuardsFaction").expect("fixture faction");
    let player = game.world_mut().player();
    game.world_mut().add_item(player, gold, 17);
    game.world_mut().faction(faction).unwrap().crime_gold_nonviolent = 25;

    assert_eq!(
        game.world_mut().item_count(player, gold),
        17,
        "fixture must begin with removable inventory"
    );
    call(
        &mut game,
        &guards,
        "SendPlayerToJail",
        &[Value::Bool(true), Value::Bool(false)],
    );

    assert_eq!(
        game.world_mut().item_count(player, gold),
        0,
        "requested inventory removal must take effect"
    );
}
