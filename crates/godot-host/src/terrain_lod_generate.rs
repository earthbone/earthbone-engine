// SPDX-License-Identifier: GPL-3.0-or-later

//! Generates a BTR terrain mesh from decoded LAND height grids. The 33 by 33
//! tile sampling and LAND-only triangle mask follow tes4skyrim's
//! `asset_convert/lod/terrain_lod.py` and `terrain_nif.py`. The output uses
//! `soe-nif`'s Skyrim SE static NIF writer, which the BTR consumer reads.

use std::collections::HashMap;

use soe_nif::blocks::{flags1, flags2, ShaderType};
use soe_nif::mesh::{Material, MaterialKind, Mesh};
use soe_nif::{Bound, Transform, Vec3};

use crate::streaming::CellCoord;
use crate::terrain::CellHeights;
use crate::terrain_lod::LodBlockCoord;
use crate::units::CELL_UNITS;

const SIDE: usize = 33;
const INTERVALS: usize = SIDE - 1;

fn nearest_height<S: std::hash::BuildHasher>(
    cells: &HashMap<CellCoord, CellHeights, S>,
    x: i64,
    y: i64,
) -> f32 {
    let selected = cells.iter().min_by_key(|(cell, _)| {
        let west = i64::from(cell.x) * INTERVALS as i64;
        let south = i64::from(cell.y) * INTERVALS as i64;
        let dx = (west - x).max(0).max(x - west - INTERVALS as i64);
        let dy = (south - y).max(0).max(y - south - INTERVALS as i64);
        (
            i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy),
            -i64::from(cell.y),
            -i64::from(cell.x),
        )
    });
    let Some((cell, heights)) = selected else {
        return 0.0;
    };
    let col = (x - i64::from(cell.x) * INTERVALS as i64).clamp(0, INTERVALS as i64) as usize;
    let row = (y - i64::from(cell.y) * INTERVALS as i64).clamp(0, INTERVALS as i64) as usize;
    heights.units[row * SIDE + col]
}

/// Build one block's BTR bytes from authored LAND heights. Cells beyond the
/// block's north and east edges may be supplied so those boundary samples
/// agree with the neighboring block. Missing LAND is edge extended for
/// vertex heights but produces no triangles. A block without LAND returns
/// `None`.
pub fn generate_terrain_lod_btr<S: std::hash::BuildHasher>(
    cells: &HashMap<CellCoord, CellHeights, S>,
    block: LodBlockCoord,
) -> soe_nif::Result<Option<Vec<u8>>> {
    if cells.values().any(|cell| {
        cell.units.len() != SIDE * SIDE || cell.units.iter().any(|height| !height.is_finite())
    }) {
        return Err(soe_nif::Error::InvalidData {
            block: 0,
            reason: "LAND height grid must have 1089 finite samples".to_owned(),
        });
    }
    let level = i64::from(block.level.cells());
    let x0 = i64::from(block.x);
    let y0 = i64::from(block.y);
    if x0 + level > i64::from(i32::MAX) || y0 + level > i64::from(i32::MAX) {
        return Err(soe_nif::Error::InvalidData {
            block: 0,
            reason: "LOD block extends beyond the cell coordinate range".to_owned(),
        });
    }
    let has_land = (0..level).any(|cy| {
        (0..level).any(|cx| cells.contains_key(&CellCoord::new((x0 + cx) as i32, (y0 + cy) as i32)))
    });
    if !has_land {
        return Ok(None);
    }

    let mut positions = Vec::with_capacity(SIDE * SIDE);
    let mut uvs = Vec::with_capacity(SIDE * SIDE);
    for row in 0..SIDE {
        for col in 0..SIDE {
            let x = (x0 * INTERVALS as i64) + col as i64 * level;
            let y = (y0 * INTERVALS as i64) + row as i64 * level;
            positions.push([
                col as f32 * CELL_UNITS * level as f32 / INTERVALS as f32,
                row as f32 * CELL_UNITS * level as f32 / INTERVALS as f32,
                nearest_height(cells, x, y),
            ]);
            uvs.push([
                col as f32 / INTERVALS as f32,
                1.0 - row as f32 / INTERVALS as f32,
            ]);
        }
    }

    let mut indices = Vec::with_capacity(INTERVALS * INTERVALS * 6);
    for row in 0..INTERVALS {
        for col in 0..INTERVALS {
            let cx = x0 + col as i64 * level / INTERVALS as i64;
            let cy = y0 + row as i64 * level / INTERVALS as i64;
            if !cells.contains_key(&CellCoord::new(cx as i32, cy as i32)) {
                continue;
            }
            let sw = (row * SIDE + col) as u32;
            let se = sw + 1;
            let nw = sw + SIDE as u32;
            let ne = nw + 1;
            indices.extend_from_slice(&[sw, se, ne, sw, ne, nw]);
        }
    }
    if indices.is_empty() {
        return Ok(None);
    }
    let material = Material {
        kind: MaterialKind::Lighting(ShaderType::LodLandscapeNoise),
        flags1: flags1::MODEL_SPACE_NORMALS | flags1::ZBUFFER_TEST,
        flags2: flags2::LOD_LANDSCAPE | flags2::ZBUFFER_WRITE,
        ..Material::default()
    };
    let mesh = Mesh {
        name: "land".to_owned(),
        block: 0,
        transform: Transform::IDENTITY,
        bound: Bound {
            center: Vec3::new(0.0, 0.0, 0.0),
            radius: 0.0,
        },
        positions,
        normals: Vec::new(),
        tangents: Vec::new(),
        uv_sets: vec![uvs.clone()],
        uvs,
        colors: Vec::new(),
        indices,
        bone_weights: Vec::new(),
        bone_indices: Vec::new(),
        skin: None,
        material,
    };
    soe_nif::static_mesh::export_nif(&[mesh]).map(Some)
}
