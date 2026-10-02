// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::hud::Snapshot;

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Ui06Adversarial.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

#[test]
fn ui_06_meter_fill_stays_bounded_when_debuff_pushes_value_below_zero() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.damage_actor_value(player, "health", 40.0).unwrap();
        world
            .mod_temporary_actor_value(player, "health", -80.0)
            .unwrap();
    }

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    assert_eq!(snapshot.health.current, -20.0);
    assert_eq!(snapshot.health.maximum, 20.0);
    assert_eq!(snapshot.health.fraction, 0.0);
    assert!((0.0..=1.0).contains(&snapshot.health.fraction));
}

#[test]
fn ui_06_zero_maximum_does_not_produce_a_nonfinite_meter_fill() {
    let mut game = game();
    let player = game.player();
    game.world_mut()
        .mod_temporary_actor_value(player, "stamina", -100.0)
        .unwrap();

    let snapshot = Snapshot::read(&mut game, player, 0.0);

    assert_eq!(snapshot.stamina.current, 0.0);
    assert_eq!(snapshot.stamina.maximum, 0.0);
    assert_eq!(snapshot.stamina.fraction, 0.0);
    assert!(snapshot.stamina.fraction.is_finite());
}

#[test]
fn ui_06_compass_centers_each_of_the_eight_heading_directions() {
    let mut game = game();
    let player = game.player();
    let directions = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

    for (index, direction) in directions.into_iter().enumerate() {
        let yaw = index as f32 * std::f32::consts::FRAC_PI_4;
        let snapshot = Snapshot::read(&mut game, player, yaw);
        assert!(
            snapshot.compass.contains(&format!("[{direction}]")),
            "expected {direction} centered for yaw {yaw}, got {}",
            snapshot.compass
        );
    }
}
