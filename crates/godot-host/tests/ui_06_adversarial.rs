// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::hud::Snapshot;

#[test]
fn ui_06_restore_and_maximum_changes_are_reflected_on_the_next_hud_read() {
    let mut order = LoadOrder::new();
    order
        .add("Ui06Breaker.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.damage_actor_value(player, "health", 80.0).unwrap();
        world.mod_temporary_actor_value(player, "health", 50.0).unwrap();
    }

    let damaged = Snapshot::read(&mut game, player, 0.0);
    assert_eq!((damaged.health.current, damaged.health.maximum), (70.0, 150.0));

    game.world_mut()
        .restore_actor_value(player, "health", 80.0)
        .unwrap();
    let restored = Snapshot::read(&mut game, player, 0.0);
    assert_eq!((restored.health.current, restored.health.maximum), (150.0, 150.0));
    assert_eq!(restored.health.fraction, 1.0);
}
