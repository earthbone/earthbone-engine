// SPDX-License-Identifier: GPL-3.0-or-later

//! Exterior block selection and LAND construction, from CELL and WRLD
//! records in the load order. Parent terrain follows CommonLib's kUseLandData.

use soe_esm::records::{cell::Cell, land::Land, wrld::World};
use soe_esm::{tag, GlobalFormId, Record};
use std::collections::HashMap;

use super::{
    DataSource, DataSourceError, ExteriorBlock, ExteriorCell, LoadedCell, LoadedExterior,
    LoadedSurface, SurfaceTextures,
};
use crate::mesh::{build_surface, MaterialDesc, MeshData, MeshError, MeshOptions, SurfaceArrays};

fn within_radius(position: (i32, i32), origin: (i32, i32), radius: i32) -> bool {
    u32::try_from(radius).is_ok_and(|radius| {
        position.0.abs_diff(origin.0) <= radius && position.1.abs_diff(origin.1) <= radius
    })
}

/// A cell's LAND heights as one surface in Skyrim units, with 32 by 32 quads.
/// Faces are wound counter-clockwise from above so build_surface's winding
/// flip matches NIF faces. VNML normals are used when present and generated
/// otherwise. Returns None when the record has no height block.
fn terrain_surface(cell: &ExteriorCell, land: &Land) -> Result<Option<SurfaceArrays>, MeshError> {
    use soe_esm::records::land::VERTS_PER_SIDE;
    let Some(heights) = &land.heights else {
        return Ok(None);
    };
    let step = crate::units::CELL_UNITS / (VERTS_PER_SIDE - 1) as f32;
    let base_x = cell.x as f32 * crate::units::CELL_UNITS;
    let base_y = cell.y as f32 * crate::units::CELL_UNITS;
    let decoded = heights.decode();
    let mut positions = Vec::with_capacity(decoded.len());
    let mut uvs = Vec::with_capacity(decoded.len());
    for (i, &h) in decoded.iter().enumerate() {
        let row = i / VERTS_PER_SIDE;
        let col = i % VERTS_PER_SIDE;
        positions.push([base_x + col as f32 * step, base_y + row as f32 * step, h]);
        uvs.push([
            col as f32 / (VERTS_PER_SIDE - 1) as f32,
            1.0 - row as f32 / (VERTS_PER_SIDE - 1) as f32,
        ]);
    }
    let normals = land.normals.as_ref().map(|n| {
        n.iter()
            .map(|&[x, y, z]| {
                [
                    f32::from(x) / 127.0,
                    f32::from(y) / 127.0,
                    f32::from(z) / 127.0,
                ]
            })
            .collect()
    });
    let side = VERTS_PER_SIDE as u32;
    let mut indices = Vec::with_capacity((VERTS_PER_SIDE - 1) * (VERTS_PER_SIDE - 1) * 6);
    for row in 0..side - 1 {
        for col in 0..side - 1 {
            let sw = row * side + col;
            let se = sw + 1;
            let nw = sw + side;
            let ne = nw + 1;
            indices.extend_from_slice(&[sw, se, ne, sw, ne, nw]);
        }
    }
    let data = MeshData {
        positions,
        normals,
        tangents: None,
        uvs: Some(uvs),
        indices,
        bone_indices: None,
        bone_weights: None,
        material: MaterialDesc::default(),
    };
    build_surface(&data, MeshOptions::default()).map(Some)
}

impl DataSource {
    /// The square of exterior cells within `radius` grid steps of the cell
    /// named `editor_id`, in that cell's worldspace, sorted south to north
    /// then west to east. Radius 2 is the game's `uGridsToLoad=5`. One pass
    /// over the winning `CELL` records, which is cheap next to the reference
    /// scan since there are about fifty thousand cells against a million
    /// references. Fails when the editor id names no exterior cell, or when
    /// `radius` is negative. A negative radius is invalid input, and is
    /// rejected here with `DataSourceError::InvalidRadius`.
    pub fn resolve_exterior_block(
        &self,
        editor_id: &str,
        radius: i32,
    ) -> Result<ExteriorBlock, DataSourceError> {
        if radius < 0 {
            return Err(DataSourceError::InvalidRadius(radius));
        }
        let _span = soe_trace::Span::new("host.scan_cells", "host");
        let mut exteriors: Vec<(GlobalFormId, i32, i32, GlobalFormId)> = Vec::new();
        let mut origin: Option<(GlobalFormId, i32, i32)> = None;
        for (id, rec) in self.order.winners_of(tag::CELL) {
            let Some(raw_world) = rec.placement.world else {
                continue;
            };
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let localized = self.order.file(rref.file).plugin.header.is_localized();
            let Ok(view) = Cell::from_record(rec, localized) else {
                continue;
            };
            let Some(grid) = view.grid else {
                continue;
            };
            let world = self.order.resolve(rref.file, raw_world);
            exteriors.push((world, grid.x, grid.y, id));
            if view.editor_id.as_deref() == Some(editor_id) {
                origin = Some((world, grid.x, grid.y));
            }
        }
        let Some((world, ox, oy)) = origin else {
            return Err(DataSourceError::CellNotFound(format!(
                "{editor_id} (no exterior cell with a grid position has that editor id)"
            )));
        };
        let in_range = |x: i32, y: i32| within_radius((x, y), (ox, oy), radius);
        let mut cells: Vec<ExteriorCell> = exteriors
            .iter()
            .filter(|&&(w, x, y, _)| w == world && in_range(x, y))
            .map(|&(_, x, y, id)| ExteriorCell { x, y, id })
            .collect();
        cells.sort_by_key(|c| (c.y, c.x));
        let land_parent = self.land_parent_of(world);
        let mut parent_cells: Vec<ExteriorCell> = land_parent
            .map(|parent| {
                exteriors
                    .iter()
                    .filter(|&&(w, x, y, _)| w == parent && in_range(x, y))
                    .map(|&(_, x, y, id)| ExteriorCell { x, y, id })
                    .collect()
            })
            .unwrap_or_default();
        parent_cells.sort_by_key(|c| (c.y, c.x));
        Ok(ExteriorBlock {
            world,
            world_editor_id: self.order.winner(world).and_then(Record::editor_id),
            origin: (ox, oy),
            radius,
            cells,
            land_parent,
            parent_cells,
        })
    }

    /// The worldspace this one takes its terrain from: its `WNAM` parent
    /// when the `PNAM` "use land data" bit is set (CommonLib's
    /// `kUseLandData`). `WhiterunWorld` carries its own `LAND` for three of
    /// the nine cells around its origin and draws Tamriel's under the rest,
    /// which is what that bit means in practice.
    pub(super) fn land_parent_of(&self, world: GlobalFormId) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(world)?;
        let rec = self.order.record(rref);
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        let view = World::from_record(rec, localized).ok()?;
        let uses_parent_land = view.parent_flags.is_some_and(|f| f & 0x0001 != 0);
        let parent = view.parent.filter(|_| uses_parent_land)?;
        Some(self.order.resolve(rref.file, parent))
    }

    /// The winning `LAND` record of each cell in `cells`, one pass over the
    /// `LAND` winners. A cell without one (the worldspace's border, a city
    /// worldspace's unused squares) is simply absent from the result.
    fn land_for_cells(
        &self,
        cells: &[ExteriorCell],
    ) -> HashMap<GlobalFormId, (GlobalFormId, Land)> {
        let _span = soe_trace::Span::new("host.scan_land", "host");
        let wanted: std::collections::HashSet<GlobalFormId> = cells.iter().map(|c| c.id).collect();
        let mut out = HashMap::new();
        for (id, rec) in self.order.winners_of(tag::LAND) {
            let Some(raw_cell) = rec.placement.cell else {
                continue;
            };
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let cell_id = self.order.resolve(rref.file, raw_cell);
            if !wanted.contains(&cell_id) {
                continue;
            }
            match Land::from_record(rec) {
                Ok(land) => {
                    out.insert(cell_id, (id, land));
                }
                Err(e) => eprintln!("stream_exterior: LAND {id}: {e}"),
            }
        }
        out
    }

    /// Streams every cell of the block around `editor_id` and appends each
    /// cell's terrain as one more surface, so the Godot layer treats land like
    /// any other geometry. A cell without its own `LAND` takes the parent
    /// worldspace's at the same grid position when the worldspace inherits
    /// land data. Terrain carries no texture yet: the LAND quadrant layers
    /// (`LTEX` base plus alpha layers) need a splat material the host does
    /// not have, so it draws in the untextured material.
    pub fn stream_exterior(
        &self,
        editor_id: &str,
        radius: i32,
    ) -> Result<LoadedExterior, DataSourceError> {
        let _span = soe_trace::Span::new("host.stream_exterior", "host");
        let block = self.resolve_exterior_block(editor_id, radius)?;
        self.stream_exterior_block(block)
    }

    /// Stream an already resolved exterior selection, including inherited terrain.
    ///
    /// Materializes every cell's `LoadedCell` into one `Vec` before
    /// returning, which is what the CPU benchmark and the integration test
    /// need to inspect the whole block at once. `stream_exterior_each`
    /// below is the low-peak-memory sibling: it hands each cell to a
    /// callback and drops it before moving to the next, which is what the
    /// Godot host uses since it converts and uploads each cell right away
    /// and has no reason to hold all of them in Rust memory at once.
    pub(super) fn stream_exterior_block(
        &self,
        block: ExteriorBlock,
    ) -> Result<LoadedExterior, DataSourceError> {
        let mut land_cells = 0;
        let mut cells = Vec::with_capacity(block.cells.len());
        let result = self.stream_exterior_each(&block, |cell, loaded, has_land| {
            if has_land {
                land_cells += 1;
            }
            cells.push((cell, loaded));
        });
        result.map(|()| LoadedExterior {
            block,
            cells,
            land_cells,
        })
    }

    /// Streams the cells of `block` one at a time, in grid order, calling
    /// `on_cell` with each `ExteriorCell`, its `LoadedCell` (terrain surface
    /// included when the cell has one) and whether a terrain surface was
    /// added, as soon as the cell is ready, then dropping it. The caller
    /// decides what "done with a cell" means, immediate conversion and
    /// RenderingServer upload for the Godot host, so the block's raw
    /// geometry and texture buffers never all sit in memory at the same
    /// time. This is the fix for PRF-02: the previous `stream_exterior_block`
    /// built the whole 9-cell `Vec` first and the Godot layer only read it
    /// afterward, so a full block's decoded meshes and textures (roughly
    /// 700 MiB for the Whiterun radius 1 block, measured headless) stayed
    /// resident throughout the RenderingServer upload loop on top of
    /// whatever the upload itself held.
    ///
    /// Public, not `pub(crate)`, so a headless integration test can call
    /// it directly: `CellStreamerNode::stream_exterior` in `godot_layer.rs`
    /// is a GDExtension `#[func]` reachable only from a live Godot process
    /// and cannot be exercised by this crate's test suite, but it is a
    /// thin wrapper over this method plus the RenderingServer upload. This
    /// keeps the streaming contract itself (call order, one cell alive at
    /// a time, totals matching the non-streaming path) under test even
    /// though the RenderingServer half stays untested outside a live run.
    pub fn stream_exterior_each(
        &self,
        block: &ExteriorBlock,
        mut on_cell: impl FnMut(ExteriorCell, LoadedCell, bool),
    ) -> Result<(), DataSourceError> {
        let mut wanted = block.cells.clone();
        wanted.extend_from_slice(&block.parent_cells);
        let land = self.land_for_cells(&wanted);
        let parent_at = |x: i32, y: i32| {
            block
                .parent_cells
                .iter()
                .find(|c| c.x == x && c.y == y)
                .map(|c| c.id)
        };
        for cell in &block.cells {
            let mut loaded = self.stream_cell_id(cell.id)?;
            let mut has_land = false;
            let own_or_parent = land
                .get(&cell.id)
                .or_else(|| parent_at(cell.x, cell.y).and_then(|id| land.get(&id)));
            if let Some((land_id, land)) = own_or_parent {
                let _terrain = soe_trace::Span::new("host.terrain_surface", "host");
                match terrain_surface(cell, land) {
                    Ok(Some(arrays)) => {
                        has_land = true;
                        loaded.stats.triangles += arrays.triangle_count();
                        loaded.surfaces.push(LoadedSurface {
                            reference: *land_id,
                            arrays,
                            textures: SurfaceTextures::default(),
                        });
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("stream_exterior: LAND {land_id}: {e}"),
                }
            }
            on_cell(*cell, loaded, has_land);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::within_radius;

    #[test]
    fn grid_distance_handles_extreme_coordinates_and_negative_radii() {
        assert!(!within_radius((i32::MIN, 0), (i32::MAX, 0), i32::MAX));
        assert!(!within_radius((0, i32::MAX), (0, i32::MIN), i32::MAX));
        assert!(!within_radius((0, 0), (0, 0), -1));
        assert!(within_radius((i32::MIN, i32::MAX), (i32::MIN, i32::MAX), 0));
        assert!(within_radius((4, -4), (2, -2), 2));
        assert!(!within_radius((5, -4), (2, -2), 2));
    }
}
