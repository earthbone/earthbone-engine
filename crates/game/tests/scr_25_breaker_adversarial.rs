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
    let faction = game.form("GuardsFaction").expect("fixture faction");
    let player = game.world_mut().player();
    game.world_mut()
        .actor(witness)
        .expect("resident guard actor")
        .factions
        .insert(faction, 0);

    call(&mut game, &guards, "SendAssaultAlarm", &[]);

    assert!(
        game.world_mut().actor_is_alarmed(witness),
        "the nearby NPC must behave as if the player assaulted them"
    );
    assert!(game
        .world_mut()
        .actor(witness)
        .unwrap()
        .witnessed_crimes
        .contains(&player));
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
    game.world_mut()
        .faction(faction)
        .unwrap()
        .crime_gold_nonviolent = 25;

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

#[test]
fn scr_25_jail_default_arguments_remove_inventory_and_request_real_jail() {
    let mut game = game();
    let guards = obj(&mut game, "GuardsFaction");
    let gold = game.form("Gold001").expect("fixture gold");
    let faction = game.form("GuardsFaction").expect("fixture faction");
    let player = game.world_mut().player();
    game.world_mut().add_item(player, gold, 3);
    game.world_mut()
        .faction(faction)
        .unwrap()
        .crime_gold_nonviolent = 1;

    call(&mut game, &guards, "SendPlayerToJail", &[]);

    assert_eq!(game.world_mut().item_count(player, gold), 0);
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_25_jail_false_real_jail_option_keeps_player_in_current_cell() {
    let mut game = game();
    let guards = obj(&mut game, "GuardsFaction");
    let faction = game.form("GuardsFaction").expect("fixture faction");
    let player = game.world_mut().player();
    let original_cell = game.world().loaded_cell();
    game.world_mut()
        .faction(faction)
        .unwrap()
        .crime_gold_nonviolent = 1;

    call(
        &mut game,
        &guards,
        "SendPlayerToJail",
        &[Value::Bool(false), Value::Bool(false)],
    );

    assert_eq!(game.world().loaded_cell(), original_cell);
    assert_eq!(
        game.world().reference_ref(player).unwrap().cell,
        original_cell
    );
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}
