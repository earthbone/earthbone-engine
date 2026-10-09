// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell-granular BTR handoff to loaded LAND, using xEdit's cell/block layout.
//! OpenMW's `components/terrain/buffercache.cpp` connects different edge
//! resolutions with triangle fans. Archived BTR triangles are irregular, so
//! here each triangle is clipped to cell squares before its exposed edges
//! are subdivided at LAND samples and connected with a fan.

use std::collections::HashMap;

use crate::mesh::{Aabb, SurfaceArrays};
use crate::streaming::CellCoord;
use crate::terrain_lod::LodBlockCoord;
use crate::units::{GodotPos, CELL_METRES, LAND_SAMPLES};

const EPSILON: f32 = 0.001;

/// Invalid geometry supplied to the terrain handoff.
#[derive(Debug, thiserror::Error, PartialEq)]
#[error("invalid LOD join input: {0}")]
pub struct JoinError(pub &'static str);

#[derive(Clone, Copy)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
    tangent: [f32; 4],
}

fn lerp<const N: usize>(a: [f32; N], b: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

impl Vertex {
    fn between(self, other: Self, t: f32) -> Self {
        Self {
            position: lerp(self.position, other.position, t),
            normal: lerp(self.normal, other.normal, t),
            uv: lerp(self.uv, other.uv, t),
            color: lerp(self.color, other.color, t),
            tangent: lerp(self.tangent, other.tangent, t),
        }
    }
}

fn clip(vertices: &[Vertex], axis: usize, edge: f32, sign: f32) -> Vec<Vertex> {
    let mut out = Vec::new();
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let da = (a.position[axis] - edge) * sign;
        let db = (b.position[axis] - edge) * sign;
        if da >= 0.0 {
            out.push(a);
        }
        if (da < 0.0) != (db < 0.0) {
            let mut v = a.between(b, da / (da - db));
            v.position[axis] = edge;
            out.push(v);
        }
    }
    out
}

fn edge_height(heights: &[f32], col: f32, row: f32) -> f32 {
    let col = col.clamp(0.0, 32.0);
    let row = row.clamp(0.0, 32.0);
    let c = col.floor() as usize;
    let r = row.floor() as usize;
    let sample = |c: usize, r: usize| heights[r * LAND_SAMPLES + c];
    let a = sample(c, r);
    let b = sample((c + 1).min(32), r);
    let d = sample(c, (r + 1).min(32));
    let e = sample((c + 1).min(32), (r + 1).min(32));
    let south = a + (b - a) * col.fract();
    let north = d + (e - d) * col.fract();
    south + (north - south) * row.fract()
}

fn match_height<S: std::hash::BuildHasher>(v: &mut Vertex, near: &HashMap<CellCoord, Vec<f32>, S>) {
    // CONTEXT: Sorted coordinate candidates make a shared corner deterministic.
    let x = (v.position[0] / CELL_METRES).round() as i32;
    let y = (-v.position[2] / CELL_METRES).round() as i32;
    for cy in [y - 1, y] {
        for cx in [x - 1, x] {
            let Some(heights) = near.get(&CellCoord::new(cx, cy)) else {
                continue;
            };
            let col = (v.position[0] / CELL_METRES - cx as f32) * 32.0;
            let row = (-v.position[2] / CELL_METRES - cy as f32) * 32.0;
            let inside = (-0.001..=32.001).contains(&col) && (-0.001..=32.001).contains(&row);
            let boundary = col.abs() < 0.001
                || (col - 32.0).abs() < 0.001
                || row.abs() < 0.001
                || (row - 32.0).abs() < 0.001;
            if inside && boundary {
                v.position[1] = edge_height(heights, col, row);
                return;
            }
        }
    }
}

fn subdivide<S: std::hash::BuildHasher>(
    vertices: &[Vertex],
    cell: CellCoord,
    near: &HashMap<CellCoord, Vec<f32>, S>,
) -> Vec<Vertex> {
    let mut out = Vec::new();
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        out.push(a);
        for (axis, edge, neighbour) in [
            (
                0,
                cell.x as f32 * CELL_METRES,
                CellCoord::new(cell.x - 1, cell.y),
            ),
            (
                0,
                (cell.x + 1) as f32 * CELL_METRES,
                CellCoord::new(cell.x + 1, cell.y),
            ),
            (
                2,
                -cell.y as f32 * CELL_METRES,
                CellCoord::new(cell.x, cell.y - 1),
            ),
            (
                2,
                -(cell.y + 1) as f32 * CELL_METRES,
                CellCoord::new(cell.x, cell.y + 1),
            ),
        ] {
            if !near.contains_key(&neighbour)
                || (a.position[axis] - edge).abs() > EPSILON
                || (b.position[axis] - edge).abs() > EPSILON
            {
                continue;
            }
            let along = if axis == 0 { 2 } else { 0 };
            let delta = b.position[along] - a.position[along];
            if delta.abs() <= EPSILON {
                continue;
            }
            let start = if along == 0 {
                cell.x as f32 * CELL_METRES
            } else {
                -(cell.y + 1) as f32 * CELL_METRES
            };
            let mut points = Vec::new();
            for sample in 1..32 {
                let position = start + sample as f32 * CELL_METRES / 32.0;
                let t = (position - a.position[along]) / delta;
                if t > 0.00001 && t < 0.99999 {
                    let mut v = a.between(b, t);
                    v.position[along] = position;
                    points.push((t, v));
                }
            }
            points.sort_by(|a, b| a.0.total_cmp(&b.0));
            out.extend(points.into_iter().map(|(_, v)| v));
        }
    }
    for v in &mut out {
        match_height(v, near);
    }
    out
}

/// Cut a validated, unskinned BTR surface into cell squares, remove loaded
/// LAND cells, and match adjoining edges to their 33 by 33 height samples.
/// `near` heights are Godot Y metres in LAND south-to-north row order.
/// Water uses the same coverage mask but retains its recorded plane height.
///
/// # Errors
/// Returns an error for malformed arrays, height grids or block coordinates.
pub fn join_surface<S: std::hash::BuildHasher>(
    surface: &SurfaceArrays,
    block: LodBlockCoord,
    near: &HashMap<CellCoord, Vec<f32>, S>,
) -> Result<SurfaceArrays, JoinError> {
    let n = surface.positions.len();
    let valid_len = |len: usize, stride: usize| len == 0 || len == n * stride;
    if !surface.indices.len().is_multiple_of(3)
        || surface.indices.iter().any(|&i| i < 0 || i as usize >= n)
        || !valid_len(surface.normals.len(), 1)
        || !valid_len(surface.uvs.len(), 1)
        || !valid_len(surface.colors.len(), 1)
        || !valid_len(surface.tangents.len(), 4)
        || surface.is_skinned()
        || !surface.weights.is_empty()
        || surface.positions.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(JoinError("surface arrays"));
    }
    if near
        .values()
        .any(|h| h.len() != LAND_SAMPLES * LAND_SAMPLES || h.iter().any(|v| !v.is_finite()))
    {
        return Err(JoinError("LAND height grid"));
    }
    let span = block.level.cells();
    let Some(end_x) = block.x.checked_add(span) else {
        return Err(JoinError("block extent"));
    };
    let Some(end_y) = block.y.checked_add(span) else {
        return Err(JoinError("block extent"));
    };
    if block.x == i32::MIN || block.y == i32::MIN || end_x == i32::MAX || end_y == i32::MAX {
        return Err(JoinError("block extent"));
    }
    let touches_near = near.keys().any(|cell| {
        let x = i64::from(cell.x);
        let y = i64::from(cell.y);
        x >= i64::from(block.x) - 1
            && x <= i64::from(end_x)
            && y >= i64::from(block.y) - 1
            && y <= i64::from(end_y)
    });
    if !touches_near {
        return Ok(surface.clone());
    }
    let mut out = surface.clone();
    out.positions.clear();
    out.normals.clear();
    out.uvs.clear();
    out.colors.clear();
    out.tangents.clear();
    out.indices.clear();
    let vertex = |i: usize| Vertex {
        position: surface.positions[i],
        normal: surface.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
        uv: surface.uvs.get(i).copied().unwrap_or_default(),
        color: surface.colors.get(i).copied().unwrap_or([1.0; 4]),
        tangent: surface
            .tangents
            .get(i * 4..i * 4 + 4)
            .and_then(|t| t.try_into().ok())
            .unwrap_or([1.0, 0.0, 0.0, 1.0]),
    };
    for triangle in surface.indices.as_chunks::<3>().0 {
        let triangle = [
            vertex(triangle[0] as usize),
            vertex(triangle[1] as usize),
            vertex(triangle[2] as usize),
        ];
        let min_x = triangle
            .iter()
            .map(|v| (v.position[0] / CELL_METRES).floor() as i32)
            .min()
            .unwrap_or(block.x)
            .max(block.x);
        let max_x = triangle
            .iter()
            .map(|v| (v.position[0] / CELL_METRES).floor() as i32)
            .max()
            .unwrap_or(block.x)
            .min(end_x - 1);
        let min_y = triangle
            .iter()
            .map(|v| (-v.position[2] / CELL_METRES).floor() as i32)
            .min()
            .unwrap_or(block.y)
            .max(block.y);
        let max_y = triangle
            .iter()
            .map(|v| (-v.position[2] / CELL_METRES).floor() as i32)
            .max()
            .unwrap_or(block.y)
            .min(end_y - 1);
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                let cell = CellCoord::new(cx, cy);
                if near.contains_key(&cell) {
                    continue;
                }
                let mut polygon = triangle.to_vec();
                for (axis, edge, sign) in [
                    (0, cx as f32 * CELL_METRES, 1.0),
                    (0, (cx + 1) as f32 * CELL_METRES, -1.0),
                    (2, -cy as f32 * CELL_METRES, -1.0),
                    (2, -(cy + 1) as f32 * CELL_METRES, 1.0),
                ] {
                    polygon = clip(&polygon, axis, edge, sign);
                }
                if polygon.len() < 3 {
                    continue;
                }
                // CONTEXT: Keep the centre at its BTR height while the edge joins LAND.
                let mut center = polygon[0];
                for (i, v) in polygon.iter().enumerate().skip(1) {
                    center = center.between(*v, 1.0 / (i + 1) as f32);
                }
                if surface.material.water.is_none() {
                    polygon = subdivide(&polygon, cell, near);
                }
                for i in 0..polygon.len() {
                    let a = polygon[i];
                    let b = polygon[(i + 1) % polygon.len()];
                    let area = (a.position[0] - center.position[0])
                        * (b.position[2] - center.position[2])
                        - (a.position[2] - center.position[2])
                            * (b.position[0] - center.position[0]);
                    if area.abs() < 0.000001 {
                        continue;
                    }
                    for v in [center, a, b] {
                        out.indices.push(out.positions.len() as i32);
                        out.positions.push(v.position);
                        out.normals.push(v.normal);
                        if !surface.uvs.is_empty() {
                            out.uvs.push(v.uv);
                        }
                        if !surface.colors.is_empty() {
                            out.colors.push(v.color);
                        }
                        if !surface.tangents.is_empty() {
                            out.tangents.extend(v.tangent);
                        }
                    }
                }
            }
        }
    }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in &out.positions {
        for i in 0..3 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    if !out.positions.is_empty() {
        out.bounds = Aabb {
            min: GodotPos::new(min[0], min[1], min[2]),
            max: GodotPos::new(max[0], max[1], max[2]),
        };
    }
    Ok(out)
}
