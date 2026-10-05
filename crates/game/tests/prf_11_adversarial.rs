// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial installed NPC planning frame check for PRF-11.

use crate::common::installed;
use soe_esm::tag;
use soe_game::{ai, Game};

#[test]
fn prf_11_adversarial_bannered_mare_ai_stays_under_frame_budget() {
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
    .unwrap();
    let mut game = Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .unwrap();
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
    assert!(actors.len() >= 5, "only {} actors", actors.len());
    ai::nav_graph(&mut game.world_mut());

    let limit = std::time::Duration::from_millis(8);
    let mut worst = std::time::Duration::ZERO;
    for frame in 0..3600 {
        game.tick(1.0 / 60.0);
        let elapsed = game.world().ai.planning_time;
        worst = worst.max(elapsed);
        assert!(
            elapsed <= limit,
            "frame {frame}: {elapsed:?} exceeded {limit:?}"
        );
        assert!(
            game.world().ai.evaluation_time <= elapsed,
            "evaluation time exceeded the complete AI tick at frame {frame}"
        );
    }
    assert!(
        game.world().ai.evaluations >= actors.len(),
        "NPC packages were not evaluated"
    );
    eprintln!(
        "PRF-11 adversarial Bannered Mare: {} NPCs, 3600 frames, worst AI {worst:?}",
        actors.len()
    );
}
