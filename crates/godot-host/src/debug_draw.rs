// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional debug geometry from winning NAVM records and live actor routes.
//! xEdit wbNVNM and wbNavmeshCoverFlags define links and ledges. Godot's local
//! ImmediateMesh documentation supplies the rendering path on a dedicated layer.

use soe_esm::records::navm::{Geometry, NavMesh};
use soe_esm::{GlobalFormId, Tag};
use soe_game::{debug_views::Views, World};

pub mod scenegraph;
#[cfg(test)]
mod tests;

/// Layer 20 is reserved for optional debug geometry.
pub const DEBUG_LAYER: u32 = 1 << 19;

/// A primitive carries its colour independently of neighbouring flags.
#[derive(Debug, Clone, PartialEq)]
pub struct Primitive<const N: usize> {
    pub points: [[f32; 3]; N],
    pub color: [f32; 4],
}

/// Counts describe the exact geometry handed to the Godot debug mesh.
#[derive(Debug, Default)]
pub struct Frame {
    pub triangles: Vec<Primitive<3>>,
    pub lines: Vec<Primitive<2>>,
    pub navmesh_triangles: usize,
    pub path_segments: usize,
    pub waypoints: usize,
    pub edge_links: usize,
    pub door_links: usize,
    pub teleport_links: usize,
    pub cover_edges: usize,
    pub highlighted: usize,
    pub text: Vec<String>,
}

/// Loaded authored geometry, retained only while a debug view is enabled.
pub struct LoadedNavmesh {
    pub form: GlobalFormId,
    pub cell: GlobalFormId,
    pub graph_index: u32,
    pub geometry: Geometry,
}

/// Select attached records before decoding and match live meshes by cell and geometry.
pub fn loaded_navmeshes(world: &World) -> Vec<LoadedNavmesh> {
    let mut used = std::collections::BTreeSet::new();
    world
        .order()
        .winners_of(Tag(*b"NAVM"))
        .filter_map(|(form, _)| {
            let record = world.record(form)?;
            let owner = record
                .placement
                .cell
                .and_then(|raw| world.resolve_from(form, raw));
            if owner.is_some_and(|cell| !world.cell_is_attached(cell)) {
                return None;
            }
            let geometry = NavMesh::from_record(record).ok()?.geometry?;
            let cell = owner.or_else(|| {
                if let soe_esm::records::navm::Parent::Cell(raw) = geometry.parent {
                    world.resolve_from(form, raw)
                } else {
                    None
                }
            })?;
            if !world.cell_is_attached(cell) {
                return None;
            }
            let graph_index = world.ai.nav_graph().and_then(|graph| {
                world
                    .ai
                    .nav_mesh_cells()
                    .iter()
                    .enumerate()
                    .find_map(|(index, owner)| {
                        if *owner != Some(cell) || used.contains(&index) {
                            return None;
                        }
                        let mesh = graph.meshes().get(index)?;
                        (mesh.vertices == geometry.vertices
                            && mesh.triangles.len() == geometry.triangles.len()
                            && mesh.triangles.iter().zip(&geometry.triangles).all(
                                |(live, authored)| {
                                    live.vertices == authored.vertices.map(u32::from)
                                },
                            ))
                        .then_some(index)
                    })
            });
            if let Some(index) = graph_index {
                used.insert(index);
            }
            Some(LoadedNavmesh {
                form,
                cell,
                graph_index: graph_index.map_or(u32::MAX, |index| index as u32),
                geometry,
            })
        })
        .collect()
}

const YELLOW: [f32; 4] = [1.0, 0.9, 0.0, 1.0];
const CYAN: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
const MAGENTA: [f32; 4] = [1.0, 0.1, 1.0, 1.0];

/// Build nothing when all switches are off. Coordinates stay in Skyrim units
/// until upload so tests compare them directly with records and actor state.
pub fn frame(
    world: &mut World,
    views: &Views,
    navmeshes: &[LoadedNavmesh],
    player: GlobalFormId,
) -> Frame {
    let mut out = Frame::default();
    if !views.drawing() {
        return out;
    }
    if views.navmesh {
        draw_navmeshes(&mut out, world, navmeshes, player);
    }
    if views.paths {
        draw_paths(&mut out, world);
    }
    if let Some(reference) = views.pivot.and_then(|id| world.ref_placement(id)) {
        cross(&mut out, reference.position, 24.0, YELLOW);
    }
    if views.ai_text {
        out.text.extend(soe_game::debug_views::ai_list(world, None));
    }
    out
}

fn triangle_color(flags: u16, cover: u16, highlight: bool, blocked: bool) -> [f32; 4] {
    if highlight {
        return [1.0, 1.0, 0.0, 0.65];
    }
    if blocked || flags & 8 != 0 {
        return [0.9, 0.1, 0.1, 0.45];
    }
    if flags & 0x0200 != 0 {
        return [0.1, 0.4, 1.0, 0.4];
    }
    if flags & 0x0400 != 0 {
        return [1.0, 0.1, 0.9, 0.4];
    }
    if cover & 15 == 2 || (cover >> 6) & 15 == 2 {
        return [1.0, 0.5, 0.0, 0.4];
    }
    if flags & 0x0040 != 0 {
        return [0.1, 1.0, 0.3, 0.4];
    }
    [0.5, 0.7, 0.7, 0.2]
}

fn centroid(geometry: &Geometry, triangle: usize) -> Option<[f32; 3]> {
    let triangle = geometry.triangles.get(triangle)?;
    let [a, b, c] = triangle.vertices.map(|v| geometry.vertices[usize::from(v)]);
    Some(std::array::from_fn(|axis| {
        (a[axis] + b[axis] + c[axis]) / 3.0
    }))
}

fn draw_navmeshes(out: &mut Frame, world: &World, meshes: &[LoadedNavmesh], player: GlobalFormId) {
    let graph = world.ai.nav_graph();
    let player_triangle = under_player(world, meshes, player);
    for mesh in meshes {
        for (index, triangle) in mesh.geometry.triangles.iter().enumerate() {
            let highlighted = player_triangle == Some((mesh.form, index));
            let blocked = graph
                .and_then(|g| g.meshes().get(mesh.graph_index as usize))
                .and_then(|m| m.triangles.get(index))
                .is_some_and(|t| t.blocked);
            let color = triangle_color(triangle.flags, triangle.cover_flags, highlighted, blocked);
            let points = triangle
                .vertices
                .map(|v| mesh.geometry.vertices[usize::from(v)]);
            out.triangles.push(Primitive { points, color });
            out.navmesh_triangles += 1;
            out.highlighted += usize::from(highlighted);
            for edge in 0..3 {
                let a = points[edge];
                let b = points[(edge + 1) % 3];
                line(out, a, b, color);
                if edge < 2 && (triangle.cover_flags >> (edge * 6)) & 15 != 0 {
                    line(out, a, b, YELLOW);
                    out.cover_edges += 1;
                }
                if triangle.flags & (1 << edge) == 0 {
                    continue;
                }
                let Some(link) = usize::try_from(triangle.edges[edge])
                    .ok()
                    .and_then(|i| mesh.geometry.edge_links.get(i))
                else {
                    continue;
                };
                let Some(target_form) = world.resolve_from(mesh.form, link.navmesh) else {
                    continue;
                };
                let from = std::array::from_fn(|axis| a[axis].midpoint(b[axis]));
                let to =
                    if let Some(target) = meshes.iter().find(|target| target.form == target_form) {
                        let Some(point) = usize::try_from(link.triangle)
                            .ok()
                            .and_then(|index| centroid(&target.geometry, index))
                        else {
                            continue;
                        };
                        point
                    } else {
                        if world.record(target_form).is_none() {
                            continue;
                        }
                        out.text.push(format!(
                        "edge link {} -> {target_form} triangle={} kind={} outside loaded cells",
                        mesh.form, link.triangle, link.kind
                    ));
                        [from[0], from[1], from[2] + 64.0]
                    };
                line(out, from, to, CYAN);
                out.edge_links += 1;
            }
        }
        for door_link in &mesh.geometry.door_links {
            let Some(from) = usize::try_from(door_link.triangle)
                .ok()
                .and_then(|i| centroid(&mesh.geometry, i))
            else {
                continue;
            };
            let Some(door) = world.resolve_from(mesh.form, door_link.door) else {
                continue;
            };
            let Some(placement) = world.ref_placement(door) else {
                continue;
            };
            line(out, from, placement.position, MAGENTA);
            cross(out, placement.position, 16.0, MAGENTA);
            out.door_links += 1;
            if let Some(destination) = world
                .reference_ref(door)
                .and_then(|r| r.teleport.map(|(destination, _)| destination))
            {
                out.teleport_links += 1;
                let to = if world.reference_ref(destination).and_then(|r| r.cell) == Some(mesh.cell)
                {
                    world
                        .ref_placement(destination)
                        .map_or(placement.position, |p| p.position)
                } else {
                    [
                        placement.position[0],
                        placement.position[1],
                        placement.position[2] + 96.0,
                    ]
                };
                line(out, placement.position, to, CYAN);
                out.text.push(format!(
                    "teleport {door} -> {destination} cell={:?}",
                    world.reference_ref(destination).and_then(|r| r.cell)
                ));
            }
        }
    }
}

fn under_player(
    world: &World,
    meshes: &[LoadedNavmesh],
    player: GlobalFormId,
) -> Option<(GlobalFormId, usize)> {
    let position = world.ref_placement(player)?.position;
    let cell = world.reference_ref(player)?.cell?;
    let scope = world.cell_coordinate_space(cell);
    meshes
        .iter()
        .filter(|mesh| world.cell_coordinate_space(mesh.cell) == scope)
        .flat_map(|mesh| {
            mesh.geometry
                .triangles
                .iter()
                .enumerate()
                .filter_map(move |(index, triangle)| {
                    let points = triangle
                        .vertices
                        .map(|v| mesh.geometry.vertices[usize::from(v)]);
                    let height = height_at(points, position)?;
                    Some(((position[2] - height).abs(), mesh.form, index))
                })
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)))
        .map(|(_, form, index)| (form, index))
}

// CONTEXT: Mesh::height_at in soe-navmesh supplies this projection, including blocked surfaces here.
fn height_at([a, b, c]: [[f32; 3]; 3], [x, y, _]: [f32; 3]) -> Option<f32> {
    let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if determinant.abs() < 1e-9 {
        return None;
    }
    let w0 = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / determinant;
    let w1 = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / determinant;
    let w2 = 1.0 - w0 - w1;
    if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
        return None;
    }
    let height = w0 * a[2] + w1 * b[2] + w2 * c[2];
    height.is_finite().then_some(height)
}

fn draw_paths(out: &mut Frame, world: &mut World) {
    for id in world.loaded_actors() {
        let target_id = soe_game::ai::debug_target(world, id);
        let package_destination =
            soe_game::ai::debug_destination(world, id).map(|place| place.position);
        let (Some(actor), Some(placement)) = (world.actor_ref(id), world.ref_placement(id)) else {
            continue;
        };
        if world
            .reference_ref(id)
            .and_then(|r| r.cell)
            .is_none_or(|cell| !world.cell_is_attached(cell))
        {
            continue;
        }
        let state = soe_game::debug_views::path_state(world, id);
        let color = match state {
            "failed" => [1.0, 0.1, 0.1, 1.0],
            "partial" | "one point" => YELLOW,
            _ => CYAN,
        };
        let mut from = placement.position;
        for point in &actor.ai.path {
            line(out, from, *point, color);
            cross(out, *point, 8.0, color);
            out.path_segments += 1;
            out.waypoints += 1;
            from = *point;
        }
        if let Some(query) = world.ai.debug_query(id) {
            cross(out, query.target, 28.0, YELLOW);
            out.text.push(format!("query {id}: {query:?}"));
        }
        if let Some(destination) = actor.ai.destination {
            cross(out, destination, 20.0, MAGENTA);
            line(out, placement.position, destination, MAGENTA);
        }
        if let Some(center) = package_destination.or(actor.ai.center) {
            cross(out, center, 24.0, [0.1, 1.0, 0.3, 1.0]);
        }
        if let Some(target) = target_id.and_then(|target| world.ref_placement(target)) {
            cross(out, target.position, 32.0, YELLOW);
        }
        out.text.push(format!(
            "{id} {:?}: {state}, {} points, target={:?}, destination={:?}, package_destination={:?}",
            actor.ai.procedure,
            actor.ai.path.len(),
            target_id,
            actor.ai.destination,
            package_destination.or(actor.ai.center).or(actor.ai.destination)
        ));
    }
}

fn line(out: &mut Frame, from: [f32; 3], to: [f32; 3], color: [f32; 4]) {
    out.lines.push(Primitive {
        points: [from, to],
        color,
    });
}

fn cross(out: &mut Frame, position: [f32; 3], size: f32, color: [f32; 4]) {
    for axis in 0..3 {
        let mut from = position;
        let mut to = position;
        from[axis] -= size;
        to[axis] += size;
        line(out, from, to, color);
    }
}

#[cfg(feature = "godot")]
pub(crate) mod host;
