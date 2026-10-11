// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Adversarial cold and sustained planning budget check for PRF-11.

use crate::common::installed;
use soe_esm::tag;
use soe_game::Game;
use soe_navmesh::{NavGraph, NodeId};
use soe_trace::{Operation, WorkFrame};
use std::task::Poll;

fn grid(size: u32) -> NavGraph {
    let mut graph = NavGraph::new();
    let vertices = (0..=size)
        .flat_map(|y| (0..=size).map(move |x| [x as f32, y as f32, 0.0]))
        .collect();
    let mut triangles = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let a = y * (size + 1) + x;
            triangles.push([a, a + 1, a + size + 2]);
            triangles.push([a, a + size + 2, a + size + 1]);
        }
    }
    graph.add_mesh_with_adjacency(vertices, &triangles);
    graph
}

#[test]
fn prf_11_breaker4_large_mesh_location_has_bounded_triangle_work() {
    let graph = grid(200);
    assert_eq!(graph.triangle_count(), 80_000);
    for point in [[100.2, 110.7, 0.0], [0.0, 0.0, 0.0], [100.0, 100.0, 0.0]] {
        let frame = WorkFrame::begin();
        assert!(graph.locate(point).is_some());
        let tested = frame.counts().get(Operation::NavmeshTriangles);
        assert!(tested <= 8, "tested {tested} triangles of 80,000");
    }
}

#[test]
fn prf_11_breaker4_locate_handles_invalid_points_overlaps_and_blocked_floors() {
    let mut graph = NavGraph::new();
    let vertices = vec![
        [0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [0.0, 100.0, 0.0],
        [0.0, 0.0, 20.0],
        [100.0, 0.0, 20.0],
        [0.0, 100.0, 20.0],
    ];
    graph.add_mesh_with_adjacency(vertices, &[[0, 1, 2], [3, 4, 5]]);
    assert_eq!(graph.locate([20.0, 20.0, 10.0]), Some(NodeId::new(0, 0)));
    assert_eq!(graph.locate([20.0, 20.0, 20.0]), Some(NodeId::new(0, 1)));
    assert_eq!(graph.locate([f32::NAN, 20.0, 0.0]), None);
    assert_eq!(graph.locate([200.0, 200.0, 0.0]), None);
    graph.set_blocked(NodeId::new(0, 1), true);
    assert_eq!(graph.locate([20.0, 20.0, 20.0]), Some(NodeId::new(0, 0)));
}

#[test]
fn prf_11_breaker4_sliced_search_matches_synchronous_route_and_cancels_on_change() {
    let mut graph = grid(24);
    let from = [0.2, 0.7, 0.0];
    let to = [23.8, 23.3, 0.0];
    let expected = graph.find_path_between(from, to).expect("route exists");
    for budget in [1, 7, 64] {
        let mut search = graph.start_path_between(from, to).expect("search starts");
        assert_eq!(search.advance(&graph, 0), Poll::Pending);
        let mut completed = None;
        for _ in 0..20_000 {
            match search.advance(&graph, budget) {
                Poll::Pending => assert!(search.last_work() <= budget),
                Poll::Ready(path) => {
                    completed = path;
                    break;
                }
            }
        }
        assert_eq!(completed, Some(expected.clone()));
    }

    let mut interrupted = graph
        .start_path_between(from, to)
        .expect("search starts before graph change");
    assert_eq!(interrupted.advance(&graph, 1), Poll::Pending);
    graph.set_blocked(NodeId::new(0, 0), true);
    assert_eq!(interrupted.advance(&graph, 1), Poll::Ready(None));
}

#[test]
fn prf_11_breaker4_cold_and_sustained_bannered_mare_planning_stays_bounded() {
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
    .expect("installed masters load");
    let mut game = Game::from_shared_order(order);
    let cell = game
        .world_mut()
        .cell_by_editor_id("WhiterunBanneredMare")
        .expect("Bannered Mare cell exists");
    game.load_cell(cell);
    let references = game.world_mut().cell_references(cell);
    for id in references {
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
    }
    assert!(game.world().ai.evaluations >= actors.len());
    eprintln!(
        "PRF-11 adversarial cold and sustained check: {} actors, 3600 frames, worst AI {worst:?}",
        actors.len()
    );
}
