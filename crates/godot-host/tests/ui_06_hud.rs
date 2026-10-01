// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::hud::Snapshot;

fn game() -> Game {
    let plugin = Plugin::new(FileHeader::default());
    let mut order = LoadOrder::new();
    order.add("UiHud.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_06_status_meters_read_health_magicka_and_stamina_from_game() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.damage_actor_value(player, "health", 20.0).unwrap();
        world.damage_actor_value(player, "magicka", 35.0).unwrap();
        world.damage_actor_value(player, "stamina", 50.0).unwrap();
    }

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    assert_eq!(
        (snapshot.health.current, snapshot.health.maximum),
        (80.0, 100.0)
    );
    assert_eq!(
        (snapshot.magicka.current, snapshot.magicka.maximum),
        (65.0, 100.0)
    );
    assert_eq!(
        (snapshot.stamina.current, snapshot.stamina.maximum),
        (50.0, 100.0)
    );
    assert_eq!(snapshot.health.fraction, 0.8);
    assert_eq!(snapshot.magicka.fraction, 0.65);
    assert_eq!(snapshot.stamina.fraction, 0.5);
}

#[test]
fn ui_06_status_meters_update_after_shared_actor_values_change() {
    let mut game = game();
    let player = game.player();
    let initial = Snapshot::read(&mut game, player, 0.0);
    assert_eq!(initial.health.fraction, 1.0);

    game.world_mut()
        .damage_actor_value(player, "health", 25.0)
        .unwrap();
    let updated = Snapshot::read(&mut game, player, 0.0);

    assert_eq!(updated.health.current, 75.0);
    assert_eq!(updated.health.maximum, 100.0);
    assert_eq!(updated.health.fraction, 0.75);
}

#[test]
fn ui_06_status_meter_uses_modified_actor_value_maximum() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.damage_actor_value(player, "health", 20.0).unwrap();
        world
            .mod_temporary_actor_value(player, "health", 20.0)
            .unwrap();
    }

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    assert_eq!(snapshot.health.current, 100.0);
    assert_eq!(snapshot.health.maximum, 120.0);
    assert_eq!(snapshot.health.fraction, 100.0 / 120.0);
}

#[test]
fn ui_06_compass_render_data_updates_with_player_heading() {
    let mut game = game();
    let player = game.player();
    let north = Snapshot::read(&mut game, player, 0.0);
    let east = Snapshot::read(&mut game, player, std::f32::consts::FRAC_PI_2);

    assert_eq!(north.compass, "W   NW   [N]   NE   E");
    assert_eq!(east.compass, "N   NE   [E]   SE   S");
}
