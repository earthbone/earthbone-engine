// SPDX-License-Identifier: GPL-3.0-or-later

//! Source-cell BTO masking, from nifly Geometry.cpp and AnvilLOD BtoBuilder.cs.
//! SSE LOD4 segments are x-major cells, with index offsets and triangle counts.

use std::collections::HashSet;

use crate::{data_source::LoadedSurface, streaming::CellCoord, terrain_lod::LodBlockCoord};

/// Retain authored triangles whose source cell has no full-model replacement.
/// Unsegmented inputs retain whole-block exclusion because ownership is unknown.
pub(crate) fn mask_surface(
    source: &LoadedSurface,
    segments: &[soe_nif::blocks::GeometrySegment],
    block: LodBlockCoord,
    full: &[CellCoord],
) -> Option<(LoadedSurface, Vec<CellCoord>)> {
    let overlap = full.iter().any(|cell| block.ring_distance(*cell) == 0);
    if block.level.cells() != 4 || segments.is_empty() || segments.len() > 16 {
        return (!overlap).then(|| (source.clone(), Vec::new()));
    }
    let full: HashSet<_> = full.iter().copied().collect();
    let mut masked = source.clone();
    masked.arrays.indices.clear();
    let mut cells = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if segment.triangles == 0 {
            continue;
        }
        let cell = CellCoord::new(block.x + index as i32 / 4, block.y + index as i32 % 4);
        if full.contains(&cell) {
            continue;
        }
        let first = segment.first_index as usize;
        let end = first + segment.triangles as usize * 3;
        let indices = source.arrays.indices.get(first..end)?;
        masked.arrays.indices.extend_from_slice(indices);
        cells.push(cell);
    }
    (!masked.arrays.indices.is_empty()).then_some((masked, cells))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_source::SurfaceTextures,
        mesh::{build_surface, MeshData, MeshOptions},
        terrain_lod::LodLevel,
    };
    use soe_nif::blocks::GeometrySegment;

    #[test]
    fn source_cell_mask_retains_overhangs_without_dropping_neighboring_mountains() {
        let data = MeshData {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            indices: vec![0, 1, 2, 3, 4, 5],
            ..MeshData::default()
        };
        let source = LoadedSurface {
            animation: None,
            reference: soe_esm::GlobalFormId { file: 0, object: 1 },
            arrays: build_surface(&data, MeshOptions::default()).unwrap(),
            textures: SurfaceTextures::default(),
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        };
        let segments = [
            GeometrySegment {
                flags: 0,
                first_index: 0,
                triangles: 1,
            },
            GeometrySegment {
                flags: 0,
                first_index: 3,
                triangles: 1,
            },
        ];
        let block = LodBlockCoord {
            level: LodLevel::L4,
            x: 12,
            y: -12,
        };
        let (masked, cells) =
            mask_surface(&source, &segments, block, &[CellCoord::new(12, -12)]).unwrap();
        assert_eq!(masked.arrays.indices, source.arrays.indices[3..]);
        assert_eq!(cells, [CellCoord::new(12, -11)]);
        assert_eq!(masked.arrays.positions, source.arrays.positions);
        let (restored, _) = mask_surface(&source, &segments, block, &[]).unwrap();
        assert_eq!(restored.arrays, source.arrays);
        assert!(mask_surface(
            &source,
            &segments,
            block,
            &[CellCoord::new(12, -12), CellCoord::new(12, -11)]
        )
        .is_none());
    }
}
