// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::hud::Snapshot;

#[test]
fn ui_06_meter_recomputes_fraction_when_maximum_changes() {
    let mut order = LoadOrder::new();
    order
        .add("Hud.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .damage_actor_value(player, "magicka", 25.0)
        .unwrap();
    let first = Snapshot::read(&mut game, player, 0.0);
    assert_eq!(first.magicka.fraction, 0.75);

    game.world_mut()
        .mod_temporary_actor_value(player, "magicka", 50.0)
        .unwrap();
    let changed = Snapshot::read(&mut game, player, 0.0);
    assert_eq!(changed.magicka.maximum, 150.0);
    assert_eq!(changed.magicka.current, 125.0);
    assert_eq!(changed.magicka.fraction, 125.0 / 150.0);
}

#[test]
fn ui_06_compass_wraps_negative_and_full_turn_headings() {
    let mut order = LoadOrder::new();
    order
        .add("Hud.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let west = Snapshot::read(&mut game, player, -std::f32::consts::FRAC_PI_2);
    let wrapped = Snapshot::read(
        &mut game,
        player,
        3.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2,
    );
    assert_eq!(west.compass, "S   SW   [W]   NW   N");
    assert_eq!(wrapped.compass, west.compass);
}

#[test]
fn ui_06_meter_clamps_depleted_values_to_an_empty_fill() {
    let mut order = LoadOrder::new();
    order
        .add("Hud.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        for name in ["health", "magicka", "stamina"] {
            world.damage_actor_value(player, name, 150.0).unwrap();
        }
    }

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    for meter in [snapshot.health, snapshot.magicka, snapshot.stamina] {
        assert_eq!(meter.current, -50.0);
        assert_eq!(meter.maximum, 100.0);
        assert_eq!(meter.fraction, 0.0);
    }
}

#[test]
fn ui_06_all_meter_maxima_and_fills_follow_temporary_modifiers() {
    let mut order = LoadOrder::new();
    order
        .add("Hud.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        for name in ["health", "magicka", "stamina"] {
            world.damage_actor_value(player, name, 50.0).unwrap();
            world.mod_temporary_actor_value(player, name, 25.0).unwrap();
        }
    }

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    for meter in [snapshot.health, snapshot.magicka, snapshot.stamina] {
        assert_eq!(meter.current, 75.0);
        assert_eq!(meter.maximum, 125.0);
        assert_eq!(meter.fraction, 0.6);
    }
}

#[test]
fn ui_06_refreshes_each_meter_after_only_its_actor_value_changes() {
    let mut order = LoadOrder::new();
    order
        .add("Hud.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();

    for (name, changed) in [("health", 0), ("magicka", 1), ("stamina", 2)] {
        game.world_mut()
            .damage_actor_value(player, name, 20.0)
            .unwrap();
        let snapshot = Snapshot::read(&mut game, player, 0.0);
        let meters = [snapshot.health, snapshot.magicka, snapshot.stamina];
        for (index, meter) in meters.into_iter().enumerate() {
            let expected = if index <= changed { 80.0 } else { 100.0 };
            assert_eq!(meter.current, expected);
            assert_eq!(meter.fraction, expected / 100.0);
        }
    }
}
