// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::hud::Snapshot;

#[test]
fn ui_06_hud_snapshot_refreshes_all_values_and_heading_from_shared_game() {
    let mut order = LoadOrder::new();
    order.add("Ui06Refresh.esm", Plugin::new(FileHeader::default())).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let first = Snapshot::read(&mut game, player, 0.0);
    assert_eq!(first.health.fraction, 1.0);
    assert_eq!(first.magicka.fraction, 1.0);
    assert_eq!(first.stamina.fraction, 1.0);

    {
        let mut world = game.world_mut();
        world.damage_actor_value(player, "health", 10.0).unwrap();
        world.damage_actor_value(player, "magicka", 20.0).unwrap();
        world.damage_actor_value(player, "stamina", 30.0).unwrap();
    }
    let updated = Snapshot::read(&mut game, player, std::f32::consts::PI);
    assert_eq!(updated.health.current, 90.0);
    assert_eq!(updated.magicka.current, 80.0);
    assert_eq!(updated.stamina.current, 70.0);
    assert!(updated.compass.contains("[S]"));
}
