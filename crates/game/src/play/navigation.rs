// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixed-step movement over the existing NAVM graph and waypoint ownership.
//! Detour findStraightPath supplies the corridor model. CK PathToReference
//! requires failure when a route cannot be built. This runner has no fallback
//! teleport. Spatial scopes follow the existing cell/worldspace form identities.

use soe_esm::{records::navm::NavMesh, GlobalFormId, Tag};
use soe_navmesh::{vanilla::Builder, NavGraph, NodeId};

use super::{append_log, resolve, tick, Destination, Report, STEP_SECONDS};
use crate::Game;

/// How far outside a triangle a route sample may fall in plan view. A route
/// that hugs a boundary edge rounds off the edge by f32 noise, about 0.015
/// units in cell 0x984A, more than the barycentric slack of the
/// NAVM point query allows on a triangle of that size.
const EDGE_SLACK: f32 = 0.05;

pub(super) fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum::<f64>()
        .sqrt() as f32
}

fn key(id: GlobalFormId) -> u64 {
    (u64::from(id.file) << 32) | u64::from(id.object)
}

fn scope(game: &Game, cell: GlobalFormId) -> Result<GlobalFormId, String> {
    let world = game.world();
    let record = world
        .record(cell)
        .filter(|record| record.tag == Tag(*b"CELL"))
        .ok_or_else(|| format!("not a cell: {cell}"))?;
    if world.cell_is_interior(cell) {
        return Ok(cell);
    }
    let source = world
        .order()
        .winner_ref(cell)
        .ok_or("cell source unavailable")?;
    Ok(record
        .placement
        .world
        .map_or(cell, |raw| world.order().resolve(source.file, raw)))
}

fn graph(game: &Game, cell: GlobalFormId) -> Result<(NavGraph, Vec<GlobalFormId>), String> {
    let wanted = scope(game, cell)?;
    let mut builder = Builder::new();
    let mut cells = Vec::new();
    {
        let world = game.world();
        let order = world.order();
        for (id, record) in order.winners_of(Tag(*b"NAVM")) {
            if record.is_deleted() {
                builder.mark_deleted(key(id));
                continue;
            }
            let Some(source) = order.winner_ref(id) else {
                continue;
            };
            let Some(cell) = record
                .placement
                .cell
                .map(|raw| order.resolve(source.file, raw))
            else {
                continue;
            };
            if scope(game, cell)? != wanted {
                continue;
            }
            let nav = NavMesh::from_record(record).map_err(|e| format!("NAVM {id}: {e}"))?;
            if let Some(geometry) = nav.geometry {
                builder.add(key(id), &geometry, |raw| {
                    Some(key(order.resolve(source.file, raw)))
                });
                cells.push(cell);
            }
        }
    }
    let (mut graph, _) = builder.build();
    refresh_doors(game, &mut graph);
    Ok((graph, cells))
}

fn refresh_doors(game: &Game, graph: &mut NavGraph) {
    let doors: Vec<_> = graph.door_keys().collect();
    for door in doors {
        let id = GlobalFormId {
            file: (door >> 32) as u16,
            object: door as u32,
        };
        if let Some(reference) = game.world_mut().reference(id) {
            graph.bind_door_footprint(door, reference.position);
            graph.set_door_open(door, reference.opened);
        }
    }
}

fn surface(graph: &NavGraph, point: [f32; 3]) -> Result<([f32; 3], NodeId), String> {
    let node = graph
        .locate(point)
        .ok_or_else(|| format!("point outside walkable NAVM: {point:?}"))?;
    let mesh = graph.mesh(node.mesh);
    let triangle = graph.triangle(node);
    let [a, b, c] = triangle
        .vertices
        .map(|vertex| mesh.vertices[vertex as usize]);
    let ab: [f32; 3] = std::array::from_fn(|i| b[i] - a[i]);
    let ac: [f32; 3] = std::array::from_fn(|i| c[i] - a[i]);
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let z = a[2] - (normal[0] * (point[0] - a[0]) + normal[1] * (point[1] - a[1])) / normal[2];
    if !z.is_finite() || (z - point[2]).abs() > 128.0 {
        return Err(format!(
            "point is more than 128 units above/below NAVM: {point:?}"
        ));
    }
    Ok(([point[0], point[1], z], node))
}

pub(super) fn query_path(
    game: &Game,
    cell: GlobalFormId,
    from: [f32; 3],
    to: [f32; 3],
) -> Result<soe_navmesh::Path, String> {
    let space = scope(game, cell)?;
    let (graph, _) = graph(game, cell)?;
    let (from, _) = surface(&graph, from)?;
    let (to, _) = surface(&graph, to)?;
    graph
        .find_path_between(from, to)
        .ok_or_else(|| format!("no walkable NAVM path in {space} from {from:?} to {to:?}"))
}

pub(super) fn query_approach(
    game: &Game,
    cell: GlobalFormId,
    from: [f32; 3],
    anchor: [f32; 3],
    max_distance: f32,
) -> Result<soe_navmesh::Path, String> {
    query_approach_inset(game, cell, from, anchor, max_distance, 0.0)
}

pub(super) fn query_approach_inset(
    game: &Game,
    cell: GlobalFormId,
    from: [f32; 3],
    anchor: [f32; 3],
    max_distance: f32,
    inset: f32,
) -> Result<soe_navmesh::Path, String> {
    if !inset.is_finite() || inset < 0.0 {
        return Err("invalid NAVM portal inset".into());
    }
    if !max_distance.is_finite() || max_distance < 0.0 || !anchor.into_iter().all(f32::is_finite) {
        return Err("invalid NAVM approach bounds".into());
    }
    let (graph, _) = graph(game, cell)?;
    let (from, _) = surface(&graph, from)?;
    let to = match surface(&graph, anchor) {
        Ok((point, _)) => point,
        Err(_) => {
            let (_, point) = graph
                .nearest_surface(anchor)
                .ok_or("no walkable NAVM approach")?;
            point
        }
    };
    if distance(to, anchor) > max_distance {
        return Err(format!(
            "no walkable NAVM approach within {max_distance} units of {anchor:?}"
        ));
    }
    let path = graph
        .find_path_between(from, to)
        .ok_or_else(|| format!("no NAVM approach path from {from:?} to {to:?}"))?;
    graph
        .path_with_portal_inset(&path, inset)
        .ok_or("invalid NAVM approach corridor".into())
}

/// Height of the corridor surface under `point`, from the path's own
/// triangles. A route segment that hugs a boundary edge rounds off the edge by
/// f32 noise, so containment allows `EDGE_SLACK` in plan view. Steep cliff
/// triangles are legal NAVM surface, so no height gap to the global nearest
/// triangle is required, only that the point stays over the corridor.
fn corridor_height(graph: &NavGraph, nodes: &[NodeId], point: [f32; 3]) -> Option<f32> {
    let mut best: Option<f32> = None;
    for node in nodes {
        let mesh = graph.mesh(node.mesh);
        let triangle = graph.triangle(*node);
        let [a, b, c] = triangle
            .vertices
            .map(|vertex| mesh.vertices[vertex as usize]);
        let edge = |p: [f32; 3], q: [f32; 3]| {
            let length = (f64::from(q[0] - p[0])).hypot(f64::from(q[1] - p[1]));
            let cross = f64::from(q[0] - p[0]) * f64::from(point[1] - p[1])
                - f64::from(q[1] - p[1]) * f64::from(point[0] - p[0]);
            if length > 0.0 {
                cross / length
            } else {
                0.0
            }
        };
        let sides = [edge(a, b), edge(b, c), edge(c, a)];
        let slack = f64::from(EDGE_SLACK);
        let inside =
            sides.iter().all(|side| *side >= -slack) || sides.iter().all(|side| *side <= slack);
        if !inside {
            continue;
        }
        let ab: [f32; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let ac: [f32; 3] = std::array::from_fn(|i| c[i] - a[i]);
        let normal = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let z = if normal[2].abs() > 1e-6 {
            a[2] - (normal[0] * (point[0] - a[0]) + normal[1] * (point[1] - a[1])) / normal[2]
        } else {
            point[2]
        };
        if z.is_finite() && best.is_none_or(|old| (z - point[2]).abs() < (old - point[2]).abs()) {
            best = Some(z);
        }
    }
    best
}

pub(super) fn navigate(
    game: &mut Game,
    destination: &Destination,
    speed: f32,
    max_ticks: u32,
    report: &mut Report,
) -> Result<(), String> {
    if !speed.is_finite() || speed <= 0.0 || speed > 600.0 || max_ticks > 36_000 {
        return Err("invalid navigation bounds".into());
    }
    let (to_cell, to) = match destination {
        Destination::Reference(name) => {
            let target = resolve(game, name)?;
            let mut world = game.world_mut();
            let reference = world
                .reference(target)
                .filter(|r| r.enabled && !r.deleted)
                .ok_or("navigation reference unavailable")?;
            (
                reference.cell.ok_or("navigation reference has no cell")?,
                reference.position,
            )
        }
        Destination::Marker(marker) => (resolve(game, &marker.cell)?, marker.position),
    };
    let player = game.player();
    let (from_cell, from) = game
        .world_mut()
        .reference(player)
        .map(|r| (r.cell, r.position))
        .ok_or("player reference unavailable")?;
    let from_cell = from_cell.ok_or("player has no cell")?;
    if scope(game, from_cell)? != scope(game, to_cell)? {
        return Err(
            "navigation crosses an unconnected interior/worldspace, activate a load door first"
                .into(),
        );
    }
    let (mut graph, cells) = graph(game, from_cell)?;
    let (from_surface, _) = surface(&graph, from)?;
    let (to, to_node) = surface(&graph, to)?;
    if cells.get(to_node.mesh as usize) != Some(&to_cell) {
        return Err(format!(
            "NAVM destination does not belong to its cell: expected {to_cell:?}, found {:?}",
            cells.get(to_node.mesh as usize)
        ));
    }
    let mut position = from_surface;
    let mut cell = from_cell;
    for elapsed in 0..=max_ticks {
        let (live_cell, live_position) = game
            .world_mut()
            .reference(player)
            .map(|r| (r.cell, r.position))
            .ok_or("player reference unavailable")?;
        if live_cell != Some(cell)
            || distance(live_position, if elapsed == 0 { from } else { position }) > 0.001
        {
            return Err("navigation interrupted by another movement request".into());
        }
        if distance(live_position, to) <= 0.001 {
            return Ok(());
        }
        if elapsed == max_ticks {
            break;
        }
        refresh_doors(game, &mut graph);
        let path = graph
            .find_path_between(position, to)
            .ok_or("no walkable NAVM path")?;
        let mut budget = speed * STEP_SECONDS as f32;
        let mut next_position = position;
        let mut next_cell = cell;
        for (point, node) in path.points.iter().zip(&path.point_nodes).skip(1) {
            let segment = distance(next_position, *point);
            if segment > budget {
                let fraction = budget / segment;
                next_position = std::array::from_fn(|axis| {
                    next_position[axis] + (point[axis] - next_position[axis]) * fraction
                });
                break;
            }
            next_position = *point;
            budget -= segment;
            next_cell = *cells
                .get(node.mesh as usize)
                .ok_or("missing NAVM cell owner")?;
            if segment > 0.001 {
                break;
            }
        }
        // A funnel segment is straight in plan view and can cut across
        // triangles at different slopes, so its interpolated height differs
        // from the mesh. Every sample must lie over a triangle of the path
        // corridor (a gap, a closed door or a layered mesh fails here) and the
        // player is placed on the mesh height.
        for fraction in [0.25, 0.5, 0.75, 1.0] {
            let sample: [f32; 3] = std::array::from_fn(|axis| {
                position[axis] + (next_position[axis] - position[axis]) * fraction
            });
            if corridor_height(&graph, &path.nodes, sample).is_none() {
                return Err(format!("NAVM route contains an off-surface link: {sample:?} is outside the path corridor, from {position:?} to {next_position:?}, path {:?}", path.points));
            }
        }
        if let Some(z) = corridor_height(&graph, &path.nodes, next_position) {
            next_position[2] = z;
        }
        {
            let mut world = game.world_mut();
            let reference = world
                .reference(player)
                .ok_or("player reference unavailable")?;
            if reference.cell != Some(cell)
                || distance(
                    reference.position,
                    if elapsed == 0 { from } else { position },
                ) > 0.001
            {
                return Err("navigation interrupted by another movement request".into());
            }
            reference.position = next_position;
            reference.cell = Some(next_cell);
        }
        let log_start = game.vm.log.entries.len();
        if cell != next_cell {
            game.change_cell(Some(next_cell));
        }
        game.update_trigger_actor_position(player, Some(next_cell), position, next_position);
        append_log(game, report, log_start);
        position = next_position;
        cell = next_cell;
        tick(game, report);
        report.trace.push(format!(
            "tick {}: navigate cell {cell} position {position:?}",
            report.ticks
        ));
    }
    Err(format!("navigation timeout after {max_ticks} ticks"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    fn overlapping_interiors() -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        for (cell, end, height) in [(0x800_u32, 80.0_f32, 0.0_f32), (0x801, 200.0, -20.0)] {
            plugin.push_top(Record::new(Tag(*b"CELL"), cell).sub(Tag(*b"DATA"), [1, 0]));
            let geometry = Geometry {
                version: 12,
                location_crc: 0,
                parent: Parent::Cell(cell),
                vertices: vec![
                    [0.0, 0.0, height],
                    [end, 0.0, height],
                    [end, end, height],
                    [0.0, end, height],
                ],
                triangles: vec![
                    Triangle {
                        vertices: [0, 1, 2],
                        edges: [-1, -1, 1],
                        flags: 0,
                        cover_flags: 0,
                    },
                    Triangle {
                        vertices: [0, 2, 3],
                        edges: [0, -1, -1],
                        flags: 0,
                        cover_flags: 0,
                    },
                ],
                edge_links: vec![],
                door_links: vec![],
                cover_triangles: vec![],
                grid: NavGrid {
                    divisor: 1,
                    size: [end, end],
                    min: [0.0, 0.0, height],
                    max: [end, end, height],
                    cells: vec![vec![0, 1]],
                },
            };
            let mut nav =
                Record::new(Tag(*b"NAVM"), cell + 0x1000).sub(Tag(*b"NVNM"), geometry.to_bytes());
            nav.placement.cell = Some(cell);
            plugin.push_top(nav);
        }
        let mut order = LoadOrder::new();
        order.add("Overlapping.esm", plugin).unwrap();
        Game::new(order)
    }

    #[test]
    fn play_path_overlapping_interiors_use_the_named_cells_navmesh() {
        let game = overlapping_interiors();
        let cell = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let from = [20.0, 40.0, 0.0];
        let to = [160.0, 40.0, -20.0];
        assert!(
            crate::ai::nav_graph(&mut game.world_mut())
                .find_path_between(from, to)
                .is_none(),
            "an unrestricted point query selects the unrelated interior"
        );
        let (_, owners) = graph(&game, cell).unwrap();
        let path = query_path(&game, cell, [20.0, 40.0, 0.0], to).unwrap();
        assert!(path.points.len() >= 2);
        assert!(path
            .point_nodes
            .iter()
            .all(|node| owners[node.mesh as usize] == cell));
        assert_eq!(path.points.first(), Some(&[20.0, 40.0, -20.0]));
        assert_eq!(path.points.last(), Some(&to));
    }
    #[test]
    fn play_path_rejects_endpoints_outside_the_named_interior() {
        let game = overlapping_interiors();
        let cell = GlobalFormId {
            file: 0,
            object: 0x800,
        };
        let error = query_path(&game, cell, [20.0, 40.0, 0.0], [160.0, 40.0, -20.0]).unwrap_err();
        assert!(error.contains("outside walkable NAVM"), "{error}");
    }
    #[test]
    fn off_mesh_anchor_approach_ends_on_the_owned_navmesh() {
        let game = overlapping_interiors();
        let cell = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let path =
            query_approach(&game, cell, [20.0, 40.0, -20.0], [210.0, 40.0, 60.0], 160.0).unwrap();
        let (graph, _) = graph(&game, cell).unwrap();
        let endpoint = *path.points.last().unwrap();
        assert!(graph.locate(endpoint).is_some());
        assert!(distance(endpoint, [210.0, 40.0, 60.0]) <= 160.0);
        assert_eq!(endpoint[2], -20.0);
    }

    #[test]
    fn off_mesh_anchor_outside_the_approach_bound_fails() {
        let game = overlapping_interiors();
        let cell = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        let error = query_approach(
            &game,
            cell,
            [20.0, 40.0, -20.0],
            [1000.0, 40.0, -20.0],
            128.0,
        )
        .unwrap_err();
        assert!(error.contains("within 128 units"), "{error}");
    }
    #[test]
    fn installed_barrel_anchor_has_a_bounded_bannered_mare_approach() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let mut order = LoadOrder::new();
        for name in [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ] {
            let bytes = std::fs::read(std::path::Path::new(&data).join(name)).unwrap();
            order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
        }
        let game = Game::new(order);
        let cell = game
            .world_mut()
            .cell_by_editor_id("WhiterunBanneredMare")
            .unwrap();
        let anchor = game
            .world_mut()
            .reference(GlobalFormId {
                file: 0,
                object: 0x060359,
            })
            .unwrap()
            .position;
        let (graph, _) = graph(&game, cell).unwrap();
        let shifted = [anchor[0], anchor[1] - 160.0, anchor[2]];
        assert!(
            query_approach(&game, cell, [150.2594, -591.6012, 68.47896], shifted, 128.0).is_err()
        );
        for target in [anchor, shifted] {
            let (_, goal) = graph.nearest_surface(target).unwrap();
            println!(
                "BARREL_SURFACE target={target:?} goal={goal:?} distance={}",
                distance(goal, target)
            );
        }
        let path =
            query_approach(&game, cell, [150.2594, -591.6012, 68.47896], anchor, 128.0).unwrap();
        let goal = *path.points.last().unwrap();
        assert!(graph.locate(goal).is_some());
        assert!(distance(goal, anchor) <= 128.0);
        println!(
            "BARREL_APPROACH anchor={anchor:?} goal={goal:?} distance={} points={}",
            distance(goal, anchor),
            path.points.len()
        );
    }
}
