// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-11 installed Bannered Mare AI planning frame limit.

use crate::common::installed;
use soe_game::{ai, Game};

#[test]
fn prf_11_bannered_mare_npcs_plan_sixty_seconds_under_eight_ms() {
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
            .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
        {
            game.world_mut().actor(id);
        }
    }
    let actors = game.world().loaded_actors();
    assert!(actors.len() >= 5, "only {} actors", actors.len());
    ai::nav_graph(&mut game.world_mut());
    let oracle: Vec<_> = game
        .world()
        .order()
        .winners_of(soe_esm::tag::REFR)
        .chain(game.world().order().winners_of(soe_esm::tag::ACHR))
        .filter_map(|(id, record)| {
            let file = game.world().order().winner_ref(id)?.file;
            (game.world().order().resolve(file, record.placement.cell?) == cell).then_some(id)
        })
        .collect();
    let mut oracle = oracle;
    oracle.sort_unstable();
    assert_eq!(ai::references_in_cell(&mut game.world_mut(), cell), oracle);
    let mut worst = std::time::Duration::ZERO;
    for frame in 0..3600 {
        game.tick(1.0 / 60.0);
        let elapsed = game.world().ai.planning_time;
        worst = worst.max(elapsed);
        assert!(
            elapsed <= std::time::Duration::from_millis(8),
            "frame {frame}: {elapsed:?}"
        );
    }
    assert!(
        game.world().ai.evaluations >= actors.len(),
        "NPC packages were not evaluated"
    );
    eprintln!(
        "PRF-11 Bannered Mare: {} NPCs, 3600 frames, 60 simulated seconds, worst AI {worst:?}",
        actors.len()
    );
}
