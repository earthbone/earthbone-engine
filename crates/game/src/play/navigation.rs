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

fn graph(game: &mut Game, cell: GlobalFormId) -> Result<(NavGraph, Vec<GlobalFormId>), String> {
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

fn refresh_doors(game: &mut Game, graph: &mut NavGraph) {
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
    let (from_surface, from_node) = surface(&graph, from)?;
    let (to, to_node) = surface(&graph, to)?;
    if cells.get(from_node.mesh as usize) != Some(&from_cell)
        || cells.get(to_node.mesh as usize) != Some(&to_cell)
    {
        return Err("NAVM endpoint does not belong to its cell".into());
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
