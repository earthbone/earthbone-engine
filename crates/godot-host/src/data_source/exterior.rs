// SPDX-License-Identifier: GPL-3.0-or-later

//! Exterior block selection and LAND construction, from CELL and WRLD
//! records in the load order. LAND texture layouts follow TES5Edit and UESP,
//! parent terrain follows CommonLib's kUseLandData.
//!
//! Each cell's terrain is meshed on its own, so two neighbouring cells only
//! meet exactly where their VHGT data already agrees. Bethesda's own LAND
//! agrees at every edge inside one worldspace by construction, but
//! `land_parent_of`'s fallback pulls a *different* worldspace's LAND in for
//! cells that have none of their own, and nothing sculpted that worldspace's
//! terrain to meet the child's. `reconcile_seams` closes the resulting gap
//! by averaging the disagreeing edge before any mesh is built. See
//! `docs/godot-host.md`'s exterior section (RND-15) for the case this fixes.

use soe_esm::records::ltex::LandTexture;
use soe_esm::records::{cell::Cell, land::Land, wrld::World};
use soe_esm::{tag, GlobalFormId, Record, Tag};
use std::collections::HashMap;
use std::sync::Arc;

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

/// A cell's decoded VHGT heights in world units, row-major from the south
/// edge, or `None` when the record has no height block.
fn decode_land_heights(land: &Land) -> Option<Vec<f32>> {
    land.heights
        .as_ref()
        .map(soe_esm::records::land::Heights::decode)
}

/// One physical vertex's copies: a cell position paired with that copy's
/// index into the cell's height array.
type VertexCopy = ((i32, i32), usize);

/// Force every shared edge between the block's loaded cells to agree
/// exactly, averaging the copies wherever they differ. Keyed by grid
/// position rather than cell id, since a cell drawing its parent
/// worldspace's LAND still needs to meet its neighbours at its own grid
/// position.
///
/// On 2026-09-23 `pan_capture.gd` showed thin sky-coloured gaps along a few
/// terrain cell edges of the `WhiterunOrigin` radius 1 block. Two causes,
/// found in that order:
///
/// 1. Only three of the block's nine cells carry their own `WhiterunWorld`
///    LAND, `land_parent_of`'s fallback fills the other six with
///    `Tamriel`'s LAND at the same grid position, and Tamriel's landform
///    there was never authored to meet WhiterunWorld's city terrain at the
///    cells where the two worldspaces' data meets.
/// 2. An earlier version of this function closed that gap with two
///    sequential passes per cell, east-west against the east neighbour then
///    north-south against the north neighbour, both mutating the same
///    `heights` map. A cell with both an east and a north neighbour has its
///    north-east corner sample on both edges, so the north-south pass would
///    re-read a value the east-west pass had already moved and overwrite it
///    again, leaving the east neighbour's own copy of that corner stale.
///    Because the two passes ran in `heights.keys()` order, an unseeded
///    `HashMap` iteration order with no cross-run guarantee, which corner
///    ended up right and which stayed wrong could also change between two
///    otherwise identical `DataSource::open` calls.
///
/// The fix groups every array index that represents the same physical
/// vertex, up to four of them at an interior block corner, before writing
/// anything, and sets the whole group to its members' average in one pass.
/// A vertex two cells already agree on averages to the same value it
/// started at, so an edge internal to one worldspace's own LAND is left
/// unchanged in practice, and a group's members never overlap another
/// group's, so the order groups are visited in cannot change the result.
fn reconcile_seams(heights: &mut HashMap<(i32, i32), Vec<f32>>) {
    use soe_esm::records::land::VERTS_PER_SIDE as SIDE;
    let side = SIDE as i64;

    // Every array index, across every present cell, that stands for the
    // same physical vertex, keyed by that vertex's coordinate in the
    // block's shared sample grid. Cell (x, y)'s local (row, col) sample
    // maps to global (x * (SIDE - 1) + col, y * (SIDE - 1) + row): the
    // shared column between (x, y) and (x + 1, y) computes the same global
    // x from both sides, and likewise for the shared row between (x, y)
    // and (x, y + 1), so a corner where both a horizontal and a vertical
    // neighbour exist collects contributions from all cells that carry it.
    let mut groups: HashMap<(i64, i64), Vec<VertexCopy>> = HashMap::new();
    for &(x, y) in heights.keys() {
        for row in 0..SIDE {
            for col in 0..SIDE {
                let gx = i64::from(x) * (side - 1) + col as i64;
                let gy = i64::from(y) * (side - 1) + row as i64;
                groups
                    .entry((gx, gy))
                    .or_default()
                    .push(((x, y), row * SIDE + col));
            }
        }
    }

    for members in groups.values() {
        if members.len() < 2 {
            continue;
        }
        let sum: f32 = members.iter().map(|&(pos, idx)| heights[&pos][idx]).sum();
        let mean = sum / members.len() as f32;
        for &(pos, idx) in members {
            if let Some(cell) = heights.get_mut(&pos) {
                cell[idx] = mean;
            }
        }
    }
}

/// One cell's already-decoded, seam-reconciled heights as one surface in
/// Skyrim units, with 32 by 32 quads. Faces are wound counter-clockwise
/// from above so build_surface's winding flip matches NIF faces. `normals`
/// are VNML triples when the record has them and are generated otherwise.
fn terrain_surface_from_heights(
    cell: &ExteriorCell,
    heights: &[f32],
    normals: Option<&[[i8; 3]]>,
) -> Result<SurfaceArrays, MeshError> {
    use soe_esm::records::land::VERTS_PER_SIDE;
    let step = crate::units::CELL_UNITS / (VERTS_PER_SIDE - 1) as f32;
    let base_x = cell.x as f32 * crate::units::CELL_UNITS;
    let base_y = cell.y as f32 * crate::units::CELL_UNITS;
    let mut positions = Vec::with_capacity(heights.len());
    let mut uvs = Vec::with_capacity(heights.len());
    for (i, &h) in heights.iter().enumerate() {
        let row = i / VERTS_PER_SIDE;
        let col = i % VERTS_PER_SIDE;
        positions.push([base_x + col as f32 * step, base_y + row as f32 * step, h]);
        uvs.push([
            col as f32 / (VERTS_PER_SIDE - 1) as f32,
            1.0 - row as f32 / (VERTS_PER_SIDE - 1) as f32,
        ]);
    }
    let normals = normals.map(|n| {
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
        colors: None,
        bone_indices: None,
        bone_weights: None,
        material: MaterialDesc::default(),
    };
    build_surface(&data, MeshOptions::default())
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
                Err(e) => {
                    soe_trace::warn!(target: "host::exterior", system = "host::exterior", "stream_exterior: LAND {id}: {e}");
                }
            }
        }
        out
    }

    fn terrain_diffuse(&self, land_id: GlobalFormId, land: &Land) -> soe_dds::Image {
        let land_ref = self.order.winner_ref(land_id);
        let mut image_for = |raw_ltex: u32| {
            let path = if raw_ltex == 0 {
                "textures/landscape/dirt02.dds".to_string()
            } else {
                let land_ref = land_ref?;
                let ltex_id = self.order.resolve(land_ref.file, raw_ltex);
                let ltex_ref = self.order.winner_ref(ltex_id)?;
                let ltex_record = self.order.record(ltex_ref);
                if ltex_record.tag != tag::LTEX {
                    return None;
                }
                let ltex = LandTexture::from_record(ltex_record).ok()?;
                let texture_set = ltex.texture_set?;
                let txst_id = self.order.resolve(ltex_ref.file, texture_set);
                let txst_ref = self.order.winner_ref(txst_id)?;
                let txst = self.order.record(txst_ref);
                if txst.tag != Tag::new(b"TXST") {
                    return None;
                }
                let diffuse = soe_esm::cursor::zstring(txst.find(Tag::new(b"TX00"))?);
                if diffuse.is_empty() {
                    return None;
                }
                diffuse
            };
            self.textures
                .decode_rgba(&self.vfs, &path)
                .map(|image| (*image).clone())
        };
        super::terrain_texture::bake(land, &mut image_for)
    }

    /// Streams every cell of the block around `editor_id` and appends each
    /// cell's terrain as one more surface, so the Godot layer treats land like
    /// any other geometry. A cell without its own `LAND` takes the parent
    /// worldspace's at the same grid position when the worldspace inherits
    /// land data. LAND quadrant base and alpha layers are sampled into one
    /// RGBA albedo texture per terrain cell.
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
        // Resolve each block cell's effective LAND (its own, or the parent
        // worldspace's at the same grid position) once, decode its heights,
        // and reconcile shared edges across the whole block before any mesh
        // is built. See `reconcile_seams` for why this block needs it.
        let mut land_ref: HashMap<(i32, i32), (GlobalFormId, &Land)> = HashMap::new();
        let mut heights: HashMap<(i32, i32), Vec<f32>> = HashMap::new();
        for cell in &block.cells {
            let own_or_parent = land
                .get(&cell.id)
                .or_else(|| parent_at(cell.x, cell.y).and_then(|id| land.get(&id)));
            if let Some((land_id, land)) = own_or_parent {
                land_ref.insert((cell.x, cell.y), (*land_id, land));
                if let Some(h) = decode_land_heights(land) {
                    heights.insert((cell.x, cell.y), h);
                }
            }
        }
        reconcile_seams(&mut heights);

        for cell in &block.cells {
            let mut loaded = self.stream_cell_id(cell.id)?;
            let mut has_land = false;
            if let Some((land_id, land)) = land_ref.get(&(cell.x, cell.y)) {
                if let Some(h) = heights.get(&(cell.x, cell.y)) {
                    let _terrain = soe_trace::Span::new("host.terrain_surface", "host");
                    match terrain_surface_from_heights(cell, h, land.normals.as_deref()) {
                        Ok(mut arrays) => {
                            let diffuse = self.terrain_diffuse(*land_id, land);
                            arrays.material.diffuse = Some(format!("terrain:{land_id}"));
                            has_land = true;
                            loaded.stats.triangles += arrays.triangle_count();
                            loaded.surfaces.push(LoadedSurface {
                                animation: None,
                                reference: *land_id,
                                arrays,
                                textures: SurfaceTextures {
                                    diffuse: Some(Arc::new(super::TextureData::Rgba8(diffuse))),
                                    ..SurfaceTextures::default()
                                },
                                placement: soe_physics::Iso::IDENTITY,
                                instance_transform: None,
                                mesh_identity: None,
                            });
                        }
                        Err(e) => {
                            soe_trace::warn!(target: "host::exterior", system = "host::exterior", "stream_exterior: LAND {land_id}: {e}");
                        }
                    }
                    let grass = self.grass_surfaces_for_cell(cell, *land_id, land, h);
                    loaded.stats.grass_placements += grass.placed;
                    loaded.stats.meshes_failed += grass.failed;
                    for surface in &grass.surfaces {
                        loaded.stats.triangles += surface.arrays.triangle_count();
                    }
                    loaded.surfaces.extend(grass.surfaces);
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
