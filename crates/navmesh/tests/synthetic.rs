// SPDX-License-Identifier: GPL-3.0-or-later

//! The plan's done check: a synthetic cell produces a navmesh whose path
//! between two points avoids a placed box.

use soe_navmesh::{generate, Agent, Scene};

const BOX_MIN: [f32; 3] = [1900.0, 800.0, 0.0];
// Lower than the agent, so the hollow inside has no headroom and Recast
// leaves nothing walkable there. The roof is a separate island above the
// climb height, which is also what the game's own generator produces.
const BOX_MAX: [f32; 3] = [2200.0, 3300.0, 100.0];

fn inside_box(p: [f32; 3], margin: f32) -> bool {
    p[0] > BOX_MIN[0] - margin
        && p[0] < BOX_MAX[0] + margin
        && p[1] > BOX_MIN[1] - margin
        && p[1] < BOX_MAX[1] + margin
}

#[test]
fn a_path_across_the_cell_walks_around_the_box() {
    let mut scene = Scene::new();
    scene.add_land(0, 0, &vec![0.0; 33 * 33]);
    scene.add_box(BOX_MIN, BOX_MAX);
    let agent = Agent::default();
    let mesh = generate(&scene, &agent).unwrap();
    assert!(
        mesh.polygon_count() > 1,
        "{} polygons",
        mesh.polygon_count()
    );

    // No ground-level polygon vertex sits inside the eroded footprint of
    // the box. Roof vertices at 100 are allowed, they are an island.
    for v in &mesh.vertices {
        assert!(
            !(inside_box(*v, agent.radius * 0.5) && v[2] < 90.0),
            "vertex in the box: {v:?}"
        );
    }

    let graph = mesh.to_graph();
    let from = [400.0, 2000.0, 0.0];
    let to = [3700.0, 2000.0, 0.0];
    let path = graph
        .find_path_between(from, to)
        .expect("a path exists around the box");
    assert!(path.nodes.len() > 2, "{:?}", path.nodes);

    // The straight line would cross the box. The path polyline does not,
    // sampled every ten units along each segment.
    let direct = (to[0] - from[0]).abs();
    assert!(
        path.cost > direct,
        "the path should be longer than the straight line"
    );
    for pair in path.points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        let steps = (len / 10.0).ceil().max(1.0) as usize;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let p = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ];
            assert!(!inside_box(p, 0.0), "the path crosses the box at {p:?}");
            assert!(p[2] < 90.0, "the path climbed onto the roof at {p:?}");
        }
    }
    let ends = (path.points[0], *path.points.last().unwrap());
    assert_eq!(ends, (from, to));
}

#[test]
fn a_wall_across_the_cell_splits_it_and_the_path_fails() {
    let mut scene = Scene::new();
    scene.add_land(0, 0, &vec![0.0; 33 * 33]);
    // A wall from edge to edge, taller than the climb.
    scene.add_box([2000.0, -100.0, 0.0], [2100.0, 4200.0, 200.0]);
    let mesh = generate(&scene, &Agent::default()).unwrap();
    let graph = mesh.to_graph();
    assert!(graph
        .find_path_between([400.0, 2000.0, 0.0], [3700.0, 2000.0, 0.0])
        .is_none());
    assert!(graph
        .find_path_between([400.0, 500.0, 0.0], [400.0, 3500.0, 0.0])
        .is_some());
}

#[test]
fn a_low_step_is_walked_over() {
    let mut scene = Scene::new();
    scene.add_land(0, 0, &vec![0.0; 33 * 33]);
    // A kerb 20 units high, under the 40 unit climb.
    scene.add_box([2000.0, -100.0, 0.0], [2100.0, 4200.0, 20.0]);
    let mesh = generate(&scene, &Agent::default()).unwrap();
    let graph = mesh.to_graph();
    assert!(graph
        .find_path_between([400.0, 2000.0, 0.0], [3700.0, 2000.0, 0.0])
        .is_some());
}
