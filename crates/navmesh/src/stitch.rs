// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated exterior cell seams, following Recast's tiled borders and xEdit's
//! NVNM edge-link layout. Splitting shared spans accommodates NAVM's single
//! neighbour per triangle edge without a second navigation representation.

use std::collections::{BTreeMap, HashMap, HashSet};

use soe_esm::records::navm::{EdgeLink, Geometry, Parent};

use crate::{Error, Navmesh, Result};

const TOLERANCE: f32 = 0.01;

/// One generated exterior cell and its NAVM FormID in the export namespace.
#[derive(Debug, Clone)]
pub struct Cell {
    pub navmesh: u32,
    pub x: i16,
    pub y: i16,
    pub mesh: Navmesh,
}

/// A stitched NVNM block and the NAVM FormID under which to write or load it.
#[derive(Debug, Clone, PartialEq)]
pub struct StitchedCell {
    pub navmesh: u32,
    pub geometry: Geometry,
}

/// Match generated cell borders, split unequal edge spans and export mutual links.
/// Inputs must come from `generate_cell` with a common scene and agent. Output
/// is sorted by FormID. XY tolerance is 0.01 units. `max_height_difference`
/// bounds the accepted surface-height discrepancy at the border, normally one
/// agent voxel height. Larger differences stay open. Matched vertices take the
/// position from the lowest FormID. Input polygons must be convex in XY.
/// This does not rewrite authored NAVM records or author a NAVI info map.
///
/// # Errors
/// Rejects duplicate identities, invalid geometry, ambiguous seams and NVNM limits.
/// The input is unchanged on failure.
pub fn stitch_cells(
    world: u32,
    cells: &[Cell],
    grid_divisor: u32,
    max_height_difference: f32,
) -> Result<Vec<StitchedCell>> {
    if !max_height_difference.is_finite() || max_height_difference < 0.0 {
        return Err(Error::Export(
            "height tolerance must be finite and nonnegative".into(),
        ));
    }
    if world == 0 || !(1..=256).contains(&grid_divisor) {
        return Err(Error::Export(
            "a nonzero world and grid divisor in 1 to 256 are required".into(),
        ));
    }
    let mut cells = cells.to_vec();
    cells.sort_by_key(|c| c.navmesh);
    let mut ids = HashSet::new();
    let mut positions = BTreeMap::new();
    for (index, cell) in cells.iter().enumerate() {
        if cell.navmesh == 0
            || !ids.insert(cell.navmesh)
            || positions
                .insert((i32::from(cell.x), i32::from(cell.y)), index)
                .is_some()
        {
            return Err(Error::Export(
                "cell coordinates and nonzero NAVM IDs must be unique".into(),
            ));
        }
        validate(&cell.mesh, cell.x, cell.y)?;
    }
    let mut seams = Vec::new();
    for (i, cell) in cells.iter().enumerate() {
        for (dx, dy, axis) in [(1, 0, 0), (0, 1, 1)] {
            if let Some(&j) = positions.get(&(i32::from(cell.x) + dx, i32::from(cell.y) + dy)) {
                let boundary = f32::from(if axis == 0 { cell.x } else { cell.y }) * 4096.0 + 4096.0;
                seams.push((i, j, axis, boundary));
            }
        }
    }
    canonicalize(&mut cells, &seams, max_height_difference)?;
    for cell in &cells {
        validate(&cell.mesh, cell.x, cell.y)?;
    }
    let mut candidates = vec![Vec::new(); cells.len()];
    for &(i, j, axis, boundary) in &seams {
        for (source, target) in [(i, j), (j, i)] {
            candidates[target].extend(
                cells[source]
                    .mesh
                    .vertices
                    .iter()
                    .filter(|p| p[axis] == boundary)
                    .copied(),
            );
        }
    }
    let mut output = Vec::with_capacity(cells.len());
    for (cell, points) in cells.iter().zip(&candidates) {
        let mesh = subdivide(&cell.mesh, points, max_height_difference);
        validate(&mesh, cell.x, cell.y)?;
        output.push(StitchedCell {
            navmesh: cell.navmesh,
            geometry: mesh.to_geometry(
                Parent::Grid {
                    world,
                    x: cell.x,
                    y: cell.y,
                },
                grid_divisor,
            )?,
        });
    }
    for (i, j, axis, boundary) in seams {
        let a = borders(&output[i].geometry, axis, boundary);
        let b = borders(&output[j].geometry, axis, boundary);
        for (at, ae, av) in &a {
            let matches: Vec<_> = b
                .iter()
                .filter(|(_, _, bv)| av[0] == bv[1] && av[1] == bv[0])
                .collect();
            if matches.len() > 1 {
                return Err(Error::Export("ambiguous generated border".into()));
            }
            if let Some((bt, be, _)) = matches.first() {
                let a_id = output[i].navmesh;
                let b_id = output[j].navmesh;
                add_link(&mut output[i].geometry, *at, *ae, b_id, *bt)?;
                add_link(&mut output[j].geometry, *bt, *be, a_id, *at)?;
            }
        }
    }
    Ok(output)
}

fn close(a: [f32; 3], b: [f32; 3], height: f32) -> bool {
    (0..2).all(|axis| (a[axis] - b[axis]).abs() <= TOLERANCE) && (a[2] - b[2]).abs() <= height
}

fn canonicalize(
    cells: &mut [Cell],
    seams: &[(usize, usize, usize, f32)],
    height: f32,
) -> Result<()> {
    let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
    for &(i, j, axis, boundary) in seams {
        for (vi, &a) in cells[i]
            .mesh
            .vertices
            .iter()
            .enumerate()
            .filter(|(_, p)| p[axis] == boundary)
        {
            for (vj, &b) in cells[j]
                .mesh
                .vertices
                .iter()
                .enumerate()
                .filter(|(_, p)| p[axis] == boundary)
            {
                if !close(a, b, height) {
                    continue;
                }
                let left = groups.iter().position(|g| g.contains(&(i, vi)));
                let right = groups.iter().position(|g| g.contains(&(j, vj)));
                match (left, right) {
                    (None, None) => groups.push(vec![(i, vi), (j, vj)]),
                    (Some(g), None) => groups[g].push((j, vj)),
                    (None, Some(g)) => groups[g].push((i, vi)),
                    (Some(a), Some(b)) if a != b => {
                        let other = groups.remove(a.max(b));
                        groups[a.min(b)].extend(other);
                    }
                    _ => {}
                }
            }
        }
    }
    for group in groups {
        for &(i, vi) in &group {
            for &(j, vj) in &group {
                if !close(
                    cells[i].mesh.vertices[vi],
                    cells[j].mesh.vertices[vj],
                    height,
                ) {
                    return Err(Error::Export(
                        "ambiguous chain of border vertices exceeds tolerance".into(),
                    ));
                }
            }
        }
        if let Some(&(i, vi)) = group.iter().min() {
            let canonical = cells[i].mesh.vertices[vi];
            for (j, vj) in group {
                cells[j].mesh.vertices[vj] = canonical;
            }
        }
    }
    Ok(())
}

fn validate(mesh: &Navmesh, x: i16, y: i16) -> Result<()> {
    let origin = [f32::from(x) * 4096.0, f32::from(y) * 4096.0];
    for p in &mesh.vertices {
        if !p.iter().all(|v| v.is_finite())
            || (0..2).any(|axis| p[axis] < origin[axis] || p[axis] > origin[axis] + 4096.0)
        {
            return Err(Error::Export(
                "generated vertex is outside its cell or nonfinite".into(),
            ));
        }
    }
    for poly in &mesh.polygons {
        if poly.len() < 3 || poly.iter().any(|&v| v as usize >= mesh.vertices.len()) {
            return Err(Error::Export("invalid generated polygon indices".into()));
        }
        let mut unique = HashSet::new();
        if poly
            .iter()
            .any(|&i| !unique.insert(point_key(mesh.vertices[i as usize])))
        {
            return Err(Error::Export("generated polygon repeats a vertex".into()));
        }
        let mut area = 0.0;
        for edge in 0..poly.len() {
            let a = mesh.vertices[poly[edge] as usize];
            let b = mesh.vertices[poly[(edge + 1) % poly.len()] as usize];
            let c = mesh.vertices[poly[(edge + 2) % poly.len()] as usize];
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if cross < 0.0 || (a[0] == b[0] && a[1] == b[1]) {
                return Err(Error::Export(
                    "generated polygons must be convex and counterclockwise".into(),
                ));
            }
            area += cross;
        }
        if area <= 0.0 {
            return Err(Error::Export("degenerate generated polygon".into()));
        }
    }
    Ok(())
}

fn point_key(p: [f32; 3]) -> [u32; 3] {
    p.map(|v| if v == 0.0 { 0 } else { v.to_bits() })
}

fn subdivide(mesh: &Navmesh, candidates: &[[f32; 3]], height: f32) -> Navmesh {
    let mut out = Navmesh {
        vertices: mesh.vertices.clone(),
        ..Navmesh::default()
    };
    let mut indices: HashMap<_, _> = out
        .vertices
        .iter()
        .enumerate()
        .map(|(i, &p)| (point_key(p), i as u32))
        .collect();
    for poly in &mesh.polygons {
        let mut boundary = Vec::new();
        for e in 0..poly.len() {
            let a = mesh.vertices[poly[e] as usize];
            let b = mesh.vertices[poly[(e + 1) % poly.len()] as usize];
            boundary.push(poly[e]);
            let delta = [b[0] - a[0], b[1] - a[1]];
            let axis = usize::from(delta[1].abs() > delta[0].abs());
            let mut cuts: Vec<_> = candidates
                .iter()
                .filter_map(|&p| {
                    let t = (p[axis] - a[axis]) / delta[axis];
                    let on_edge = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t);
                    (t > 0.0 && t < 1.0 && close(p, on_edge, height)).then_some((t, p))
                })
                .collect();
            cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
            cuts.dedup_by(|a, b| close(a.1, b.1, TOLERANCE));
            for (_, p) in cuts {
                let index = *indices.entry(point_key(p)).or_insert_with(|| {
                    let index = out.vertices.len() as u32;
                    out.vertices.push(p);
                    index
                });
                boundary.push(index);
            }
        }
        let collinear = (0..poly.len()).any(|e| {
            let a = mesh.vertices[poly[e] as usize];
            let b = mesh.vertices[poly[(e + 1) % poly.len()] as usize];
            let c = mesh.vertices[poly[(e + 2) % poly.len()] as usize];
            (b[0] - a[0]) * (c[1] - b[1]) == (b[1] - a[1]) * (c[0] - b[0])
        });
        if boundary.len() == poly.len() && !collinear {
            out.polygons.push(poly.clone());
            continue;
        }
        // CONTEXT: A centre fan retains every collinear border cut without zero-area triangles.
        let center = std::array::from_fn(|axis| {
            poly.iter()
                .map(|&i| mesh.vertices[i as usize][axis] / poly.len() as f32)
                .sum()
        });
        let center_index = out.vertices.len() as u32;
        out.vertices.push(center);
        for e in 0..boundary.len() {
            out.polygons.push(vec![
                center_index,
                boundary[e],
                boundary[(e + 1) % boundary.len()],
            ]);
        }
    }
    out
}

type Border = (usize, usize, [[f32; 3]; 2]);

fn borders(geometry: &Geometry, axis: usize, boundary: f32) -> Vec<Border> {
    let mut result = Vec::new();
    for (ti, t) in geometry.triangles.iter().enumerate() {
        for e in 0..3 {
            let a = geometry.vertices[usize::from(t.vertices[e])];
            let b = geometry.vertices[usize::from(t.vertices[(e + 1) % 3])];
            if t.edges[e] == -1 && a[axis] == boundary && b[axis] == boundary {
                result.push((ti, e, [a, b]));
            }
        }
    }
    result
}

fn add_link(
    geometry: &mut Geometry,
    triangle: usize,
    edge: usize,
    target: u32,
    target_triangle: usize,
) -> Result<()> {
    if geometry.triangles[triangle].edges[edge] != -1 {
        return Err(Error::Export(
            "generated border has more than one neighbour".into(),
        ));
    }
    let index = i16::try_from(geometry.edge_links.len())
        .map_err(|_| Error::Export("edge links exceed signed 16-bit indices".into()))?;
    geometry.triangles[triangle].edges[edge] = index;
    geometry.triangles[triangle].flags |= 1 << edge;
    geometry.edge_links.push(EdgeLink {
        kind: 0,
        navmesh: target,
        triangle: target_triangle as i16,
    });
    Ok(())
}
