// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Stress the installed Bannered Mare AI planning budget across every frame.

use crate::common::installed;
use soe_esm::tag;
use soe_game::{ai, Game};

#[test]
fn prf_11_breaker_lazy_nav_graph_build_does_not_stall_the_first_ai_frame() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .expect("installed masters");
    let mut game = Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("Bannered Mare cell");
    game.load_cell(cell);
    let ids = game.world_mut().cell_references(cell);
    for id in ids {
        let is_actor = game
            .world()
            .record(id)
            .is_some_and(|record| record.tag == tag::ACHR);
        if is_actor {
            game.world_mut().actor(id);
        }
    }

    game.tick(1.0 / 60.0);
    let elapsed = game.world().ai.planning_time;
    assert!(
        elapsed <= std::time::Duration::from_millis(8),
        "cold first AI frame took {elapsed:?} while lazily building the nav graph"
    );
}

#[test]
fn prf_11_breaker_bannered_mare_planning_time_stays_bounded_for_sixty_seconds() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .expect("installed masters");
    let mut game = Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("Bannered Mare cell");
    game.load_cell(cell);
    let ids = game.world_mut().cell_references(cell);
    for id in ids {
        if game
            .world()
            .record(id)
            .is_some_and(|record| record.tag == tag::ACHR)
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
        let elapsed = world.ai.planning_time;
        worst = worst.max(elapsed);
        assert!(
            elapsed <= limit,
            "frame {frame}: {elapsed:?} > {limit:?}; phases={:?}",
            world.ai.planning_phases
        );
        for (phase, duration) in &world.ai.planning_phases {
            assert!(
                *duration <= elapsed,
                "frame {frame}: {phase}={duration:?} > whole AI tick {elapsed:?}"
            );
        }
    }
    eprintln!(
        "PRF-11 breaker: {} NPCs, 3600 frames, worst AI {worst:?}",
        actors.len()
    );
}
