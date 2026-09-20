// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{records::navm::Geometry, tag};
use soe_navmesh::{
    generate_cell,
    stitch::{stitch_cells, Cell, StitchedCell},
    vanilla::Builder,
    Agent, Navmesh, Scene,
};

fn graph(cells: &[StitchedCell]) -> soe_navmesh::NavGraph {
    let mut builder = Builder::new();
    for cell in cells {
        let geometry = Geometry::parse(tag::NAVM, &cell.geometry.to_bytes()).unwrap();
        assert_eq!(geometry, cell.geometry);
        for t in &geometry.triangles {
            let [a, b, c] = t.vertices.map(|i| geometry.vertices[i as usize]);
            assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.0);
        }
        builder.add(u64::from(cell.navmesh), &geometry, |raw| {
            Some(u64::from(raw))
        });
    }
    let (graph, stats) = builder.build();
    assert_eq!(stats.unresolved_links, 0);
    graph
}

fn verify_links(cells: &[StitchedCell]) -> usize {
    let mut count = 0;
    for cell in cells {
        for (ti, t) in cell.geometry.triangles.iter().enumerate() {
            for e in 0..3 {
                if t.flags & (1 << e) == 0 {
                    continue;
                }
                count += 1;
                let link = cell.geometry.edge_links[t.edges[e] as usize];
                let target = cells.iter().find(|c| c.navmesh == link.navmesh).unwrap();
                let other = target.geometry.triangles[link.triangle as usize];
                let av = [
                    cell.geometry.vertices[t.vertices[e] as usize],
                    cell.geometry.vertices[t.vertices[(e + 1) % 3] as usize],
                ];
                let mut reciprocal = false;
                for oe in 0..3 {
                    if other.flags & (1 << oe) != 0 {
                        let back = target.geometry.edge_links[other.edges[oe] as usize];
                        if back.navmesh == cell.navmesh && back.triangle as usize == ti {
                            let bv = [
                                target.geometry.vertices[other.vertices[oe] as usize],
                                target.geometry.vertices[other.vertices[(oe + 1) % 3] as usize],
                            ];
                            assert_eq!(av, [bv[1], bv[0]]);
                            reciprocal = true;
                        }
                    }
                }
                assert!(reciprocal);
            }
        }
    }
    count
}

#[test]
fn ai_10_generated_neighbours_share_vertices_and_cross_in_both_axes() {
    let mut scene = Scene::new();
    for x in -1..=1 {
        for y in -1..=1 {
            scene.add_land(x, y, &vec![32.0; 33 * 33]);
        }
    }
    let agent = Agent::default();
    let cells: Vec<_> = [(10, -1, 0), (20, 0, 0), (30, 0, 1)]
        .into_iter()
        .map(|(navmesh, x, y)| Cell {
            navmesh,
            x,
            y,
            mesh: generate_cell(&scene, &agent, x, y).unwrap(),
        })
        .collect();
    let output = stitch_cells(0x3c, &cells, 4, 8.0).unwrap();
    assert!(verify_links(&output) >= 4);
    let graph = graph(&output);
    let start = [-2048.0, 2048.0, 32.0];
    let end = [2048.0, 6144.0, 32.0];
    for (a, b) in [(start, end), (end, start)] {
        let path = graph.find_path_between(a, b).unwrap();
        assert_eq!(path.points.first(), Some(&a));
        assert_eq!(path.points.last(), Some(&b));
        assert!(path.nodes.iter().any(|n| n.mesh == 1));
        assert!(path.nodes.iter().any(|n| n.mesh == 2));
    }
    let mut reversed = cells;
    reversed.reverse();
    assert_eq!(stitch_cells(0x3c, &reversed, 4, 8.0).unwrap(), output);
}

fn square(navmesh: u32, x: i16, z: f32) -> Cell {
    let o = f32::from(x) * 4096.0;
    Cell {
        navmesh,
        x,
        y: 0,
        mesh: Navmesh {
            vertices: vec![
                [o, 0.0, z],
                [o + 4096.0, 0.0, z],
                [o + 4096.0, 4096.0, z],
                [o, 4096.0, z],
            ],
            polygons: vec![vec![0, 1, 2, 3]],
            neighbors: vec![vec![None; 4]],
        },
    }
}

#[test]
fn unequal_border_subdivisions_become_mutual_full_edges() {
    let a = square(10, 0, 0.0);
    let mut b = square(20, 1, 0.0);
    b.mesh
        .vertices
        .extend([[4096.0, 2048.0, 0.0], [8192.0, 2048.0, 0.0]]);
    b.mesh.polygons = vec![vec![0, 1, 5, 4], vec![4, 5, 2, 3]];
    let output = stitch_cells(0x3c, &[a, b], 2, 8.0).unwrap();
    assert_eq!(verify_links(&output), 4);
    let graph = graph(&output);
    assert!(graph
        .find_path_between([1024.0, 1024.0, 0.0], [6144.0, 3072.0, 0.0])
        .is_some());
    for cell in output {
        for t in cell.geometry.triangles {
            let [a, b, c] = t.vertices.map(|i| cell.geometry.vertices[i as usize]);
            assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.0);
        }
    }
}

#[test]
fn gaps_heights_and_absent_neighbours_stay_unlinked() {
    for b in [square(20, 1, 64.0), square(20, 2, 0.0)] {
        let output = stitch_cells(0x3c, &[square(10, 0, 0.0), b], 1, 8.0).unwrap();
        assert_eq!(verify_links(&output), 0);
        let graph = graph(&output);
        assert!(graph
            .find_path(
                soe_navmesh::NodeId::new(0, 0),
                soe_navmesh::NodeId::new(1, 0)
            )
            .is_none());
    }
    let output = stitch_cells(0x3c, &[square(10, 0, 0.0)], 1, 8.0).unwrap();
    assert_eq!(verify_links(&output), 0);
}

#[test]
fn partial_shared_span_keeps_the_unmatched_part_open() {
    let a = square(10, 0, 0.0);
    let mut b = square(20, 1, 0.0);
    b.mesh.vertices[0][1] = 1024.0;
    b.mesh.vertices[1][1] = 1024.0;
    b.mesh.vertices[2][1] = 3072.0;
    b.mesh.vertices[3][1] = 3072.0;
    let output = stitch_cells(0x3c, &[a, b], 1, 8.0).unwrap();
    assert_eq!(verify_links(&output), 2);
    assert!(graph(&output)
        .find_path_between([1024.0, 2048.0, 0.0], [6144.0, 2048.0, 0.0])
        .is_some());
}

#[test]
fn bad_inputs_return_errors_without_mutating_the_mesh() {
    let a = square(10, 0, 0.0);
    assert!(stitch_cells(0, std::slice::from_ref(&a), 1, 8.0).is_err());
    assert!(stitch_cells(0x3c, std::slice::from_ref(&a), u32::MAX, 8.0).is_err());
    assert!(stitch_cells(0x3c, &[a.clone(), a.clone()], 1, 8.0).is_err());
    let mut bad = a.clone();
    bad.mesh.polygons[0][0] = 99;
    assert!(stitch_cells(0x3c, &[bad], 1, 8.0).is_err());
    let mut bad = a.clone();
    bad.mesh.vertices[0][2] = f32::NAN;
    assert!(stitch_cells(0x3c, &[bad], 1, 8.0).is_err());
    let original = a.mesh.clone();
    stitch_cells(0x3c, std::slice::from_ref(&a), 1, 8.0).unwrap();
    assert_eq!(a.mesh, original);
}

#[test]
fn a_wall_at_the_cell_seam_does_not_gain_a_link() {
    let mut scene = Scene::new();
    scene.add_land(0, 0, &vec![0.0; 33 * 33]);
    scene.add_land(1, 0, &vec![0.0; 33 * 33]);
    scene.add_box([4000.0, 0.0, 0.0], [4200.0, 4096.0, 512.0]);
    let cells: Vec<_> = [0, 1]
        .into_iter()
        .map(|x| Cell {
            navmesh: 10 + x as u32,
            x,
            y: 0,
            mesh: generate_cell(&scene, &Agent::default(), x, 0).unwrap(),
        })
        .collect();
    let output = stitch_cells(0x3c, &cells, 1, 8.0).unwrap();
    let graph = graph(&output);
    assert!(graph
        .find_path_between([2048.0, 2048.0, 0.0], [6144.0, 2048.0, 0.0])
        .is_none());
}

#[test]
fn a_generated_slope_crosses_the_cell_seam() {
    let mut scene = Scene::new();
    for x in 0..=1 {
        let heights: Vec<_> = (0..33 * 33)
            .map(|i| {
                let wx = (x * 4096) as f32 + (i % 33) as f32 * 128.0;
                let wy = (i / 33) as f32 * 128.0;
                wx * 0.05 + wy * 0.03
            })
            .collect();
        scene.add_land(x, 0, &heights);
    }
    let cells: Vec<_> = [0, 1]
        .into_iter()
        .map(|x| Cell {
            navmesh: 10 + x as u32,
            x,
            y: 0,
            mesh: generate_cell(&scene, &Agent::default(), x, 0).unwrap(),
        })
        .collect();
    let output = stitch_cells(0x3c, &cells, 1, 8.0).unwrap();
    assert!(verify_links(&output) >= 2);
    assert!(graph(&output)
        .find_path_between([2048.0, 2048.0, 163.84], [6144.0, 2048.0, 368.64])
        .is_some());
}

#[test]
fn four_generated_cells_share_all_sloped_corner_seams() {
    for slope in [0.0, 0.04] {
        let mut scene = Scene::new();
        for x in -1..=0 {
            for y in -1..=0 {
                let heights: Vec<_> = (0..33 * 33)
                    .map(|i| {
                        let wx = (x * 4096) as f32 + (i % 33) as f32 * 128.0;
                        let wy = (y * 4096) as f32 + (i / 33) as f32 * 128.0;
                        (wx + wy) * slope
                    })
                    .collect();
                scene.add_land(x, y, &heights);
            }
        }
        let cells: Vec<_> = [(10, -1, -1), (20, 0, -1), (30, -1, 0), (40, 0, 0)]
            .into_iter()
            .map(|(navmesh, x, y)| Cell {
                navmesh,
                x,
                y,
                mesh: generate_cell(&scene, &Agent::default(), x, y).unwrap(),
            })
            .collect();
        let output = stitch_cells(0x3c, &cells, 1, 8.0).unwrap();
        assert!(verify_links(&output) >= 8);
        let graph = graph(&output);
        for a in 0..4 {
            for b in 0..4 {
                let point = |i: usize| {
                    let x = f32::from(cells[i].x) * 4096.0 + 2048.0;
                    let y = f32::from(cells[i].y) * 4096.0 + 2048.0;
                    [x, y, (x + y) * slope]
                };
                assert!(graph.find_path_between(point(a), point(b)).is_some());
            }
        }
        let mut reversed = cells;
        reversed.reverse();
        assert_eq!(stitch_cells(0x3c, &reversed, 1, 8.0).unwrap(), output);
    }
}

#[test]
fn corner_chains_and_degenerate_polygons_are_rejected() {
    let a = square(10, 0, 0.0);
    for height in [-1.0, f32::NAN, f32::INFINITY] {
        assert!(stitch_cells(0x3c, std::slice::from_ref(&a), 1, height).is_err());
    }
    let mut vertical = a.clone();
    vertical.mesh.vertices[1] = [0.0, 0.0, 10.0];
    assert!(stitch_cells(0x3c, &[vertical], 1, 8.0).is_err());
    let mut repeated = a.clone();
    repeated.mesh.polygons[0] = vec![0, 1, 2, 0, 3];
    assert!(stitch_cells(0x3c, &[repeated], 1, 8.0).is_err());
    let mut north = square(30, 0, 12.0);
    north.y = 1;
    for v in &mut north.mesh.vertices {
        v[1] += 4096.0;
    }
    let mut northeast = square(40, 1, 6.0);
    northeast.y = 1;
    for v in &mut northeast.mesh.vertices {
        v[1] += 4096.0;
    }
    let b = square(20, 1, 6.0);
    let err = stitch_cells(0x3c, &[a, b, north, northeast], 1, 8.0).unwrap_err();
    assert!(err.to_string().contains("chain"));
}
