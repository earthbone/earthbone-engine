// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads a distant-terrain LOD block's BTR file through the same NIF and
//! surface pipeline `load_static_surfaces` uses for placed statics, keyed by
//! worldspace and grid block rather than by a `REFR`. See
//! `crate::terrain_lod` for the block coordinates, the file naming and the
//! residency budget that decides which blocks to request.

use std::collections::HashMap;

use soe_esm::records::{cell::Cell, land::Land};
use soe_esm::{tag, GlobalFormId};
use soe_nif::{Transform as NifTransform, Vec3 as NifVec3};

use super::{DataSource, DataSourceError, LoadedSurface};
use crate::mesh::{build_surface, MeshOptions};
use crate::streaming::CellCoord;
use crate::terrain::CellHeights;
use crate::terrain_lod::{LodBlockCoord, LodSettings};
use crate::units::CELL_UNITS;

/// One BTR file's content, ready for the Godot layer: its built surfaces
/// and the approximate resident size a caller should hand to
/// `terrain_lod::TerrainLodStreamer::record_bytes`, so the streamer's
/// residency budget reflects what was actually built rather than a guess
/// made before the read.
pub struct LoadedLodBlock {
    pub surfaces: Vec<LoadedSurface>,
    pub approx_bytes: u64,
}

impl DataSource {
    /// Generate a BTR from winning LAND records for one worldspace block.
    /// A child worldspace uses parent LAND at positions it does not own when
    /// its PNAM use-land bit is set. Returns `None` for a block without land.
    pub fn generate_terrain_lod_block(
        &self,
        world: GlobalFormId,
        block: LodBlockCoord,
    ) -> Result<Option<Vec<u8>>, DataSourceError> {
        let parent = self.land_parent_of(world);
        let span = i64::from(block.level.cells());
        let in_bounds = |x: i32, y: i32| {
            (i64::from(block.x)..=i64::from(block.x) + span).contains(&i64::from(x))
                && (i64::from(block.y)..=i64::from(block.y) + span).contains(&i64::from(y))
        };
        let mut selected = HashMap::new();
        for (id, rec) in self.order.winners_of(tag::CELL) {
            let Some(raw_world) = rec.placement.world else {
                continue;
            };
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let owner = self.order.resolve(rref.file, raw_world);
            if owner != world && Some(owner) != parent {
                continue;
            }
            let localized = self.order.file(rref.file).plugin.header.is_localized();
            let cell = Cell::from_record(rec, localized)?;
            if let Some(grid) = cell.grid.filter(|grid| in_bounds(grid.x, grid.y)) {
                selected.insert(id, (CellCoord::new(grid.x, grid.y), owner == world));
            }
        }
        let mut heights = HashMap::<CellCoord, (bool, CellHeights)>::new();
        for (id, rec) in self.order.winners_of(tag::LAND) {
            let Some(raw_cell) = rec.placement.cell else {
                continue;
            };
            let Some(rref) = self.order.winner_ref(id) else {
                continue;
            };
            let cell_id = self.order.resolve(rref.file, raw_cell);
            let Some(&(position, own)) = selected.get(&cell_id) else {
                continue;
            };
            let land = Land::from_record(rec)?;
            let Some(encoded) = land.heights else {
                continue;
            };
            if !own && heights.get(&position).is_some_and(|entry| entry.0) {
                continue;
            }
            heights.insert(
                position,
                (
                    own,
                    CellHeights {
                        cell: position,
                        units: encoded.decode(),
                    },
                ),
            );
        }
        let heights: HashMap<CellCoord, CellHeights> = heights
            .into_iter()
            .map(|(key, (_, value))| (key, value))
            .collect();
        Ok(crate::terrain_lod_generate::generate_terrain_lod_btr(
            &heights, block,
        )?)
    }

    /// The winning `WRLD` record whose editor id matches, ignoring ASCII
    /// case, with that record's own spelling of the editor id. The spelling
    /// names the `meshes/terrain/<edid>/` folder and the `lodsettings` file
    /// `load_lod_block` and `lod_settings` read.
    #[must_use]
    pub fn resolve_world(&self, editor_id: &str) -> Option<(GlobalFormId, String)> {
        self.order
            .winners_of(soe_esm::tag::WRLD)
            .find_map(|(id, rec)| {
                rec.editor_id()
                    .filter(|edid| edid.eq_ignore_ascii_case(editor_id))
                    .map(|edid| (id, edid))
            })
    }

    /// The worldspace's `lodsettings/<edid>.lod`, or `None` when the data
    /// has no such file or it is not the 16 byte Skyrim layout. A caller
    /// falls back to a grid anchored at cell (0, 0) and level 4 then.
    pub fn lod_settings(
        &self,
        world_editor_id: &str,
    ) -> Result<Option<LodSettings>, DataSourceError> {
        match self
            .vfs
            .read(&crate::terrain_lod::lodsettings_path(world_editor_id))
        {
            Ok(bytes) => Ok(LodSettings::parse(&bytes)),
            Err(soe_bsa::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Reads and builds one worldspace's LOD block, or `None` when the
    /// block has no BTR file. A worldspace's LOD coverage rarely fills
    /// every block at every level, Tamriel's edges and any level above the
    /// one an area was baked at are the ordinary case, so a missing file is
    /// content that is not there, not a read error. This follows the same
    /// convention `Vfs::read` already uses for a missing texture: a
    /// `soe_bsa::Error::NotFound` becomes `Ok(None)` here, any other error
    /// still propagates.
    ///
    /// `world` is the worldspace's resolved form id, recorded on each
    /// returned surface so a batched LOD triangle can be traced back to a
    /// form the way `LoadedSurface::reference` already does for a `REFR` or
    /// a `LAND`. `world_editor_id` names the worldspace whose
    /// `meshes/terrain/<edid>/` folder is read, see `crate::terrain_lod` for
    /// the exact path and its corpus sources.
    ///
    /// A BTR's vertices are local to its block: a real level 4 Tamriel
    /// block builds to x and y 0 to 4 cells with an identity NIF transform
    /// (`tests/terrain_lod_placement.rs`). Each shape's own transform is
    /// therefore composed under a translation to the block's south-west
    /// cell, which plays the part a `REFR`'s placement plays for a static.
    pub fn load_lod_block(
        &self,
        world: GlobalFormId,
        world_editor_id: &str,
        block: LodBlockCoord,
    ) -> Result<Option<LoadedLodBlock>, DataSourceError> {
        let path = crate::terrain_lod::btr_path(world_editor_id, block);
        let bytes = match self.vfs.read(&path) {
            Ok(bytes) => bytes,
            Err(soe_bsa::Error::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let loaded = self.load_lod_block_bytes(world, block, &bytes)?;
        if loaded.surfaces.is_empty() {
            Ok(None)
        } else {
            Ok(Some(loaded))
        }
    }

    /// Build generated or archived BTR bytes through the same surface path
    /// used for the on-disk block. Editors can preview a block before saving.
    pub fn load_lod_block_bytes(
        &self,
        world: GlobalFormId,
        block: LodBlockCoord,
        bytes: &[u8],
    ) -> Result<LoadedLodBlock, DataSourceError> {
        let file = soe_nif::parse(bytes)?;
        let meshes = file.meshes()?;
        if meshes.is_empty() {
            return Ok(LoadedLodBlock {
                surfaces: Vec::new(),
                approx_bytes: 0,
            });
        }
        let origin = block.origin_cell();
        let placement = NifTransform {
            translation: NifVec3::new(
                origin.x as f32 * CELL_UNITS,
                origin.y as f32 * CELL_UNITS,
                0.0,
            ),
            ..NifTransform::IDENTITY
        };
        let mut surfaces = Vec::with_capacity(meshes.len());
        let mut approx_bytes = 0u64;
        for shape in &meshes {
            approx_bytes += shape.approx_bytes();
            let data = super::mesh_data_from(shape, &placement.then(&shape.transform));
            let arrays = build_surface(&data, MeshOptions::default())?;
            let textures = self.textures.resolve(&self.vfs, &arrays.material);
            surfaces.push(LoadedSurface {
                reference: world,
                arrays,
                textures,
                placement: soe_physics::Iso::IDENTITY,
                instance_transform: None,
                mesh_identity: None,
            });
        }
        Ok(LoadedLodBlock {
            surfaces,
            approx_bytes,
        })
    }
}
