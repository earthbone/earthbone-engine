// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial installed-data timing check for PRF-11.

use crate::common::installed;
use soe_game::{ai, Game};

#[test]
fn prf_11_breaker3_bannered_mare_planning_stays_below_eight_ms_for_sixty_seconds() {
    if installed::data_dir().is_none() {
        return;
    }
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .expect("installed masters load");
    let mut game = Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("Bannered Mare cell exists");
    game.load_cell(cell);
    let ids = game.world_mut().cell_references(cell);
    for id in ids {
        if game
            .world()
            .record(id)
            .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
        {
            game.world_mut().actor(id);
        }
    }
    let actors = game.world().loaded_actors();
    assert!(actors.len() >= 5, "loaded only {} actors", actors.len());
    ai::nav_graph(&mut game.world_mut());

    let limit = std::time::Duration::from_millis(8);
    let mut worst = std::time::Duration::ZERO;
    for frame in 0..3600 {
        game.tick(1.0 / 60.0);
        let world = game.world();
        worst = worst.max(world.ai.planning_time);
        assert!(
            world.ai.planning_time <= limit,
            "frame {frame}: {:?} exceeded {limit:?}, phases={:?}",
            world.ai.planning_time,
            world.ai.planning_phases
        );
        assert!(world.ai.evaluation_time <= world.ai.planning_time);
    }
    assert!(game.world().ai.evaluations >= actors.len());
    eprintln!(
        "PRF-11 adversarial Bannered Mare: {} actors, 3600 frames, worst AI {worst:?}",
        actors.len()
    );
}
