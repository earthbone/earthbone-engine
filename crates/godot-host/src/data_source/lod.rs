// SPDX-License-Identifier: GPL-3.0-or-later

//! Reads distant terrain BTR and object BTO blocks through the same NIF and
//! surface pipeline `load_static_surfaces` uses for placed statics. Missing
//! BTO blocks can be generated from winning placements and static meshes. See
//! `crate::terrain_lod` for the block coordinates, the file naming and the
//! residency budget that decides which blocks to request. Tree LST/BTT data
//! follows xEdit Core/wbLOD.pas and is built through crate::tree_lod.

use std::collections::HashMap;

use soe_esm::plugin::flags;
use soe_esm::records::{cell::Cell, land::Land, refr::Refr, stat::Static};
use soe_esm::{tag, GlobalFormId};
use soe_nif::{Transform as NifTransform, Vec3 as NifVec3};

use super::{DataSource, DataSourceError, LoadedSurface};
use crate::mesh::{build_surface, MeshOptions};
use crate::streaming::CellCoord;
use crate::terrain::CellHeights;
use crate::terrain_lod::{LodBlockCoord, LodSettings};
use crate::units::CELL_UNITS;

/// One distant terrain or object LOD block, ready for the Godot layer: its
/// built surfaces and the approximate resident size a caller should hand to
/// `terrain_lod::TerrainLodStreamer::record_bytes`, so the streamer's
/// residency budget reflects what was actually built rather than a guess
/// made before the read.
pub struct LoadedLodBlock {
    pub surfaces: Vec<LoadedSurface>,
    pub approx_bytes: u64,
    /// Number of winning references whose authored LOD mesh contributed.
    pub authored_references: usize,
    /// Triangles in the built surfaces.
    pub triangles: usize,
    /// Triangles in source meshes before any simplification.
    pub source_triangles: usize,
    /// Approximate bytes in source meshes before any simplification.
    pub source_approx_bytes: u64,
    /// References whose full model was simplified because its MNAM mesh was
    /// absent or unavailable.
    pub simplified_references: usize,
}

impl DataSource {
    /// Inventory winning BTR, BTO or BTT paths, including loose overrides.
    /// The file's level and south-west cell are authoritative, not settings.
    pub fn scenery_lod_blocks(
        &self,
        world_editor_id: &str,
        kind: crate::terrain_lod::LodKind,
    ) -> Vec<LodBlockCoord> {
        let (folder, extension) = match kind {
            crate::terrain_lod::LodKind::Terrain => ("", "btr"),
            crate::terrain_lod::LodKind::Object => ("objects", "bto"),
            crate::terrain_lod::LodKind::Tree => ("trees", "btt"),
        };
        let world_name = world_editor_id.to_ascii_lowercase();
        let prefix = if folder.is_empty() {
            format!("meshes/terrain/{world_name}/")
        } else {
            format!("meshes/terrain/{world_name}/{folder}/")
        };
        let mut blocks = Vec::new();
        for path in self.vfs.paths().filter(|path| path.starts_with(&prefix)) {
            let Some(name) = path.strip_prefix(&prefix) else {
                continue;
            };
            let fields: Vec<_> = name.split('.').collect();
            if fields.len() != 5
                || fields[0] != world_editor_id.to_ascii_lowercase()
                || fields[4] != extension
            {
                continue;
            }
            let (Ok(level), Ok(x), Ok(y)) =
                (fields[1].parse(), fields[2].parse(), fields[3].parse())
            else {
                continue;
            };
            if let Some(level) = crate::terrain_lod::LodLevel::from_cells(level) {
                blocks.push(LodBlockCoord { level, x, y });
            }
        }
        blocks.sort_unstable_by_key(|b| (b.level.cells(), b.x, b.y));
        blocks.dedup();
        blocks
    }

    /// Read BTT references and their world's LST atlas through the shared VFS.
    /// The resulting alpha-tested quads use absolute BTT world positions.
    pub fn load_tree_lod_block(
        &self,
        world: GlobalFormId,
        world_editor_id: &str,
        block: LodBlockCoord,
    ) -> Result<Option<LoadedLodBlock>, DataSourceError> {
        let bytes = match self
            .vfs
            .read(&crate::terrain_lod::btt_path(world_editor_id, block))
        {
            Ok(bytes) => bytes,
            Err(soe_bsa::Error::NotFound { .. }) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let references = crate::tree_lod::read_references(&bytes)?;
        if references.is_empty() {
            return Ok(None);
        }
        let list = self.vfs.read(&format!(
            "meshes/terrain/{world_editor_id}/trees/{world_editor_id}.lst"
        ))?;
        let types = crate::tree_lod::read_types(&list)?;
        let atlas =
            format!("textures/terrain/{world_editor_id}/trees/{world_editor_id}treelod.dds");
        let mesh = crate::tree_lod::build_billboards(&types, &references, &atlas)?;
        let arrays = build_surface(&mesh, MeshOptions::default())?;
        let approx_bytes = (arrays.positions.len() * 32 + arrays.indices.len() * 4) as u64;
        let triangles = arrays.triangle_count();
        let textures = self.textures.resolve(&self.vfs, &arrays.material);
        Ok(Some(LoadedLodBlock {
            surfaces: vec![LoadedSurface {
                animation: None,
                reference: world,
                arrays,
                textures,
                placement: soe_physics::Iso::IDENTITY,
                instance_transform: None,
                mesh_identity: None,
            }],
            approx_bytes,
            authored_references: references.len(),
            triangles,
            source_triangles: triangles,
            source_approx_bytes: approx_bytes,
            simplified_references: 0,
        }))
    }

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
        let mut loaded = self.load_lod_block_bytes(world, block, &bytes)?;
        for surface in self.lod_water_surfaces(world, block)? {
            loaded.approx_bytes += surface.arrays.mesh_byte_count() as u64;
            loaded.triangles += surface.arrays.triangle_count();
            loaded.surfaces.push(surface);
        }
        if loaded.surfaces.is_empty() {
            Ok(None)
        } else {
            Ok(Some(loaded))
        }
    }

    /// Cell-sized distant water planes using the same XCLW and inherited
    /// DNAM heights as loaded cells. CELL and WRLD No LOD Water flags omit
    /// planes, following xEdit's `wbDefinitionsTES5.pas` definitions.
    ///
    /// # Errors
    /// Returns a record error for malformed CELL or WRLD data.
    pub fn lod_water_surfaces(
        &self,
        world: GlobalFormId,
        block: LodBlockCoord,
    ) -> Result<Vec<LoadedSurface>, DataSourceError> {
        let Some(owner) = self.order.winner_ref(world) else {
            return Ok(Vec::new());
        };
        if self.order.record(owner).tag != tag::WRLD {
            return Ok(Vec::new());
        }
        let view = soe_esm::records::wrld::World::from_record(
            self.order.record(owner),
            self.order.file(owner.file).plugin.header.is_localized(),
        )?;
        if view.flags & soe_esm::records::wrld::flags::NO_LOD_WATER != 0 {
            return Ok(Vec::new());
        }
        let (lod_world, _) = self.lod_worldspace(world).unwrap_or((world, String::new()));
        let lod_owner = self.order.winner_ref(lod_world).unwrap_or(owner);
        let lod_view = soe_esm::records::wrld::World::from_record(
            self.order.record(lod_owner),
            self.order.file(lod_owner.file).plugin.header.is_localized(),
        )?;
        let mut surfaces = Vec::new();
        for y in 0..block.level.cells() {
            for x in 0..block.level.cells() {
                let (Some(x), Some(y)) = (block.x.checked_add(x), block.y.checked_add(y)) else {
                    continue;
                };
                let Some(id) = self.cell_index().exterior_cell(world, x, y) else {
                    continue;
                };
                let Some(owner) = self.order.winner_ref(id) else {
                    continue;
                };
                let cell = Cell::from_record(
                    self.order.record(owner),
                    self.order.file(owner.file).plugin.header.is_localized(),
                )?;
                if cell.flags & soe_esm::records::cell::flags::NO_LOD_WATER != 0 {
                    continue;
                }
                if let Some(mut surface) = self.water_surface(id, &[]) {
                    let cell_height = cell
                        .water_height
                        .filter(|height| super::water::is_recorded_height(*height));
                    let inherited_height = super::water::world_water(&self.order, world).height;
                    if let Some(height) = cell_height
                        .or(lod_view
                            .lod_water_height
                            .filter(|height| super::water::is_recorded_height(*height)))
                        .or(inherited_height)
                    {
                        let y = height * crate::units::METRES_PER_UNIT;
                        for position in &mut surface.arrays.positions {
                            position[1] = y;
                        }
                    }
                    if let Some(material) = lod_view
                        .lod_water
                        .and_then(|raw| self.order.winner(self.order.resolve(lod_owner.file, raw)))
                    {
                        surface.arrays.material.water =
                            Some(super::water::water_material_from(material));
                    }
                    surface.arrays.material.water.get_or_insert_with(|| {
                        crate::mesh::WaterMaterial {
                            opacity: u8::MAX,
                            ..crate::mesh::WaterMaterial::default()
                        }
                    });
                    surfaces.push(surface);
                }
            }
        }
        Ok(surfaces)
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
                authored_references: 0,
                triangles: 0,
                source_triangles: 0,
                source_approx_bytes: 0,
                simplified_references: 0,
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
                animation: None,
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
            authored_references: 0,
            triangles: meshes.iter().map(|mesh| mesh.triangle_count()).sum(),
            source_triangles: meshes.iter().map(|mesh| mesh.triangle_count()).sum(),
            source_approx_bytes: approx_bytes,
            simplified_references: 0,
        })
    }

    /// Reads and builds one worldspace object LOD block from its BTO file.
    /// When the file is missing, generates geometry from visible distant
    /// placements. Returns `None` if neither source yields geometry. BTO
    /// geometry is grouped by worldspace and block, so each returned surface
    /// keeps the resolved worldspace as its form identity while the caller
    /// keeps resident mesh instances keyed by `LodBlockCoord`. Archived BTO
    /// shapes already carry their world translation and level scale, unlike BTR.
    pub fn load_object_lod_block(
        &self,
        world: GlobalFormId,
        world_editor_id: &str,
        block: LodBlockCoord,
    ) -> Result<Option<LoadedLodBlock>, DataSourceError> {
        let path = crate::terrain_lod::bto_path(world_editor_id, block);
        let bytes = match self.vfs.read(&path) {
            Ok(bytes) => bytes,
            Err(soe_bsa::Error::NotFound { .. }) => {
                return self.generate_object_lod_block(world, world_editor_id, block)
            }
            Err(e) => return Err(e.into()),
        };
        let file = soe_nif::parse(&bytes)?;
        let meshes = file.meshes()?;
        if meshes.is_empty() {
            return Ok(None);
        }
        let mut surfaces = Vec::with_capacity(meshes.len());
        let mut approx_bytes = 0u64;
        for shape in &meshes {
            approx_bytes += shape.approx_bytes();
            let data = super::mesh_data_from(shape, &shape.transform);
            let arrays = build_surface(&data, MeshOptions::default())?;
            let textures = self.textures.resolve(&self.vfs, &arrays.material);
            surfaces.push(LoadedSurface {
                animation: None,
                reference: world,
                arrays,
                textures,
                placement: soe_physics::Iso::IDENTITY,
                instance_transform: None,
                mesh_identity: None,
            });
        }
        Ok(Some(LoadedLodBlock {
            surfaces,
            approx_bytes,
            authored_references: 0,
            triangles: meshes.iter().map(|mesh| mesh.triangle_count()).sum(),
            source_triangles: meshes.iter().map(|mesh| mesh.triangle_count()).sum(),
            source_approx_bytes: approx_bytes,
            simplified_references: 0,
        }))
    }

    /// Builds an object LOD block from winning exterior placements when no
    /// prebuilt BTO exists. It uses the level-specific MNAM model authored on
    /// each visible distant STAT, then sends every NIF shape through the same
    /// surface and texture path as a BTO. References disabled in the winning
    /// record, references with enable parents, and persistent never-fade or
    /// full-LOD references are excluded.
    pub fn generate_object_lod_block(
        &self,
        world: GlobalFormId,
        world_editor_id: &str,
        block: LodBlockCoord,
    ) -> Result<Option<LoadedLodBlock>, DataSourceError> {
        let level_index = match block.level {
            crate::terrain_lod::LodLevel::L4 => 0,
            crate::terrain_lod::LodLevel::L8 => 1,
            crate::terrain_lod::LodLevel::L16 => 2,
            crate::terrain_lod::LodLevel::L32 => 3,
        };
        let anchor = self
            .lod_settings(world_editor_id)?
            .map_or(crate::streaming::CellCoord::new(0, 0), |s| s.sw_cell);
        let origin = block.origin_cell();
        let mut surfaces = Vec::new();
        let mut approx_bytes = 0u64;
        let mut authored_references = 0usize;
        let mut triangles = 0usize;
        let mut source_triangles = 0usize;
        let mut source_approx_bytes = 0u64;
        let mut simplified_references = 0usize;
        let mut parsed = HashMap::<String, (soe_nif::NifFile, bool)>::new();

        let index = self.cell_index();
        let mut reference_ids = Vec::new();
        let max_x = block.x + block.level.cells();
        let max_y = block.y + block.level.cells();
        for x in block.x..max_x {
            for y in block.y..max_y {
                if let Some(cell_id) = index.exterior_cell(world, x, y) {
                    reference_ids.extend_from_slice(index.references(cell_id));
                }
            }
        }
        if let Some(cell_id) = index.world_persistent_cell(world) {
            reference_ids.extend_from_slice(index.references(cell_id));
        }
        reference_ids.sort_unstable();
        reference_ids.dedup();

        for reference_id in reference_ids {
            let Some(reference_ref) = self.order.winner_ref(reference_id) else {
                continue;
            };
            let record = self.order.record(reference_ref);
            if record.tag != soe_esm::tag::REFR
                || record.is_deleted()
                || record.flags & flags::INITIALLY_DISABLED != 0
            {
                continue;
            }
            let reference = Refr::from_record(record)?;
            if reference.enable_parent.is_some() && record.flags & flags::VISIBLE_WHEN_DISTANT == 0
            {
                continue;
            }
            let Some(transform) = reference.transform else {
                continue;
            };
            let cell = transform.position;
            let cell_x = (cell[0] / CELL_UNITS).floor() as i32;
            let cell_y = (cell[1] / CELL_UNITS).floor() as i32;
            let ref_block = LodBlockCoord::containing_from(
                crate::streaming::CellCoord::new(cell_x, cell_y),
                block.level,
                anchor,
            );
            let ref_world = record
                .placement
                .world
                .map(|raw| self.order.resolve(reference_ref.file, raw));
            if ref_world != Some(world) || ref_block != block {
                continue;
            }
            let base_id = self.order.resolve(reference_ref.file, reference.base);
            let Some(base_ref) = self.order.winner_ref(base_id) else {
                continue;
            };
            let base_record = self.order.record(base_ref);
            if base_record.tag != soe_esm::tag::STAT {
                continue;
            }
            let base = Static::from_record(base_record)?;
            if !base.has_distant_lod()
                || (reference.is_persistent()
                    && (base_record.flags & soe_esm::records::stat::flags::NEVER_FADES != 0
                        || record.flags & 0x0001_0000 != 0))
            {
                continue;
            }
            let lod_path = base
                .lod_models
                .as_ref()
                .and_then(|models| models.get(level_index))
                .filter(|path| !path.is_empty())
                .map(String::as_str);
            let Some(full_model_path) = base.model.as_deref() else {
                continue;
            };
            let mut model_path = lod_path.unwrap_or(full_model_path);
            let mut use_simplified_model = lod_path.is_none();
            let mut key = model_path.to_ascii_lowercase();
            if !parsed.contains_key(&key) {
                match self.vfs.read(model_path) {
                    Ok(bytes) => {
                        parsed.insert(key.clone(), (soe_nif::parse(&bytes)?, use_simplified_model));
                    }
                    Err(soe_bsa::Error::NotFound { .. }) if !use_simplified_model => {
                        model_path = full_model_path;
                        use_simplified_model = true;
                        key = model_path.to_ascii_lowercase();
                        if !parsed.contains_key(&key) {
                            let bytes = match self.vfs.read(model_path) {
                                Ok(bytes) => bytes,
                                Err(soe_bsa::Error::NotFound { .. }) => continue,
                                Err(error) => return Err(error.into()),
                            };
                            parsed.insert(key.clone(), (soe_nif::parse(&bytes)?, true));
                        }
                    }
                    Err(soe_bsa::Error::NotFound { .. }) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            let Some((file, path_requires_simplification)) = parsed.get(&key) else {
                continue;
            };
            let meshes = file.meshes()?;
            if meshes.is_empty() {
                continue;
            }
            let reference_transform = reference_transform(
                transform.position,
                transform.rotation,
                reference.scale,
                origin,
            );
            authored_references += 1;
            let simplify = *path_requires_simplification || use_simplified_model;
            if simplify {
                simplified_references += 1;
            }
            let reduction = match block.level {
                crate::terrain_lod::LodLevel::L4 => 4,
                crate::terrain_lod::LodLevel::L8 => 8,
                crate::terrain_lod::LodLevel::L16 => 16,
                crate::terrain_lod::LodLevel::L32 => 32,
            };
            for shape in &meshes {
                source_approx_bytes += shape.approx_bytes();
                source_triangles += shape.triangle_count();
                let reduced = if simplify {
                    simplify_mesh(shape, reduction)
                } else {
                    shape.clone()
                };
                approx_bytes += reduced.approx_bytes();
                triangles += reduced.triangle_count();
                let data =
                    super::mesh_data_from(&reduced, &reference_transform.then(&reduced.transform));
                let arrays = build_surface(&data, MeshOptions::default())?;
                let textures = self.textures.resolve(&self.vfs, &arrays.material);
                surfaces.push(LoadedSurface {
                    animation: None,
                    reference: world,
                    arrays,
                    textures,
                    placement: soe_physics::Iso::IDENTITY,
                    instance_transform: None,
                    mesh_identity: None,
                });
            }
        }
        if surfaces.is_empty() {
            return Ok(None);
        }
        Ok(Some(LoadedLodBlock {
            surfaces,
            approx_bytes,
            authored_references,
            triangles,
            source_triangles,
            source_approx_bytes,
            simplified_references,
        }))
    }
}

fn simplify_mesh(mesh: &soe_nif::Mesh, stride: usize) -> soe_nif::Mesh {
    let mut simplified = mesh.clone();
    let mut remap = HashMap::<u32, u32>::new();
    let mut source_indices = Vec::new();
    let mut indices = Vec::new();
    for (triangle, source) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        if triangle % stride != 0 {
            continue;
        }
        for &index in source {
            let mapped = *remap.entry(index).or_insert_with(|| {
                let mapped = source_indices.len() as u32;
                source_indices.push(index as usize);
                mapped
            });
            indices.push(mapped);
        }
    }
    simplified.positions = source_indices
        .iter()
        .map(|&index| mesh.positions[index])
        .collect();
    simplified.normals = remap_vertices(&mesh.normals, &source_indices);
    simplified.tangents = remap_vertices(&mesh.tangents, &source_indices);
    simplified.uvs = remap_vertices(&mesh.uvs, &source_indices);
    simplified.colors = remap_vertices(&mesh.colors, &source_indices);
    simplified.bone_weights = remap_vertices(&mesh.bone_weights, &source_indices);
    simplified.bone_indices = remap_vertices(&mesh.bone_indices, &source_indices);
    simplified.indices = indices;
    simplified
}

fn remap_vertices<T: Clone>(source: &[T], indices: &[usize]) -> Vec<T> {
    if source.is_empty() {
        return Vec::new();
    }
    indices.iter().map(|&index| source[index].clone()).collect()
}

fn reference_transform(
    position: [f32; 3],
    rotation: [f32; 3],
    scale: f32,
    origin: crate::streaming::CellCoord,
) -> NifTransform {
    let (sx, cx) = rotation[0].sin_cos();
    let (sy, cy) = rotation[1].sin_cos();
    let (sz, cz) = rotation[2].sin_cos();
    let rx = soe_nif::Mat33 {
        rows: [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]],
    };
    let ry = soe_nif::Mat33 {
        rows: [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]],
    };
    let rz = soe_nif::Mat33 {
        rows: [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]],
    };
    NifTransform {
        rotation: rz.mul(&ry).mul(&rx),
        translation: NifVec3::new(
            position[0] - origin.x as f32 * CELL_UNITS,
            position[1] - origin.y as f32 * CELL_UNITS,
            position[2],
        ),
        scale,
    }
}

#[cfg(test)]
mod edt_25_tests {
    use super::*;
    use crate::data_source::fixture::{quad, NifWriter};
    use soe_esm::{tag, Entry, FileHeader, Group, GroupKind, Plugin, Record};
    use soe_nif::vertex::attr;
    use std::path::Path;

    const WORLD_ID: u32 = 0x100;
    const CELL_ID: u32 = 0x200;
    const STATIC_ID: u32 = 0x300;
    const REFERENCE_ID: u32 = 0x400;
    const CELL_X: i32 = 4;

    fn source_mesh() -> Vec<u8> {
        let mut writer = NifWriter::new(100);
        let root = writer.reserve("NiNode");
        let (vertices, triangles) = quad();
        let shape = writer.bs_tri_shape(
            "Shape",
            [0.0; 3],
            -1,
            -1,
            -1,
            attr::VERTEX | attr::UVS | attr::NORMALS,
            &vertices,
            &triangles,
            true,
        );
        let shape = writer.add("BSTriShape", shape);
        let root_bytes = writer.ni_node("Root", [0.0; 3], fixture_rotation(), 1.0, &[shape]);
        writer.fill(root, root_bytes);
        writer.roots = vec![root];
        writer.finish()
    }

    fn fixture_rotation() -> [[f32; 3]; 3] {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    fn xclc(x: i32, y: i32) -> Vec<u8> {
        [x.to_le_bytes(), y.to_le_bytes(), 0u32.to_le_bytes()].concat()
    }

    fn write_fixture(directory: &Path) {
        std::fs::create_dir_all(directory.join("meshes")).unwrap();
        std::fs::write(directory.join("meshes/source.nif"), source_mesh()).unwrap();
        let mut plugin = Plugin::new(FileHeader::default());
        let mut statics = Group::new(GroupKind::Top(tag::STAT));
        let base = plugin.push_record(
            Record::new(tag::STAT, STATIC_ID)
                .with_flags(soe_esm::records::stat::flags::HAS_DISTANT_LOD)
                .zsub(b"MODL", "meshes/source.nif"),
        );
        statics.entries.push(Entry::Record(base));
        let ordinary_base = plugin
            .push_record(Record::new(tag::STAT, STATIC_ID + 1).zsub(b"MODL", "meshes/source.nif"));
        statics.entries.push(Entry::Record(ordinary_base));

        let world =
            plugin.push_record(Record::new(tag::WRLD, WORLD_ID).zsub(tag::EDID, "Edt25World"));
        let cell = plugin.push_record(
            Record::new(tag::CELL, CELL_ID)
                .sub(b"DATA", [0u8, 0])
                .sub(b"XCLC", xclc(CELL_X, 0)),
        );
        let reference = plugin.push_record(
            Record::new(tag::REFR, REFERENCE_ID)
                .sub(b"NAME", STATIC_ID.to_le_bytes())
                .sub(
                    b"DATA",
                    [
                        (CELL_X as f32 * CELL_UNITS + 2048.0).to_le_bytes(),
                        1024f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        std::f32::consts::FRAC_PI_2.to_le_bytes(),
                    ]
                    .concat(),
                )
                .sub(b"XSCL", 2f32.to_le_bytes()),
        );
        let disabled_reference = plugin.push_record(
            Record::new(tag::REFR, REFERENCE_ID + 1)
                .with_flags(soe_esm::plugin::flags::INITIALLY_DISABLED)
                .sub(b"NAME", STATIC_ID.to_le_bytes())
                .sub(
                    b"DATA",
                    [
                        (CELL_X as f32 * CELL_UNITS + 2048.0).to_le_bytes(),
                        1024f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                    ]
                    .concat(),
                ),
        );
        let parent_enabled_reference = plugin.push_record(
            Record::new(tag::REFR, REFERENCE_ID + 2)
                .sub(b"NAME", STATIC_ID.to_le_bytes())
                .sub(
                    b"DATA",
                    [
                        (CELL_X as f32 * CELL_UNITS + 2048.0).to_le_bytes(),
                        1024f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                    ]
                    .concat(),
                )
                .sub(b"XESP", [0u8; 8]),
        );
        let ordinary_reference = plugin.push_record(
            Record::new(tag::REFR, REFERENCE_ID + 3)
                .sub(b"NAME", (STATIC_ID + 1).to_le_bytes())
                .sub(
                    b"DATA",
                    [
                        (CELL_X as f32 * CELL_UNITS + 2048.0).to_le_bytes(),
                        1024f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                        0f32.to_le_bytes(),
                    ]
                    .concat(),
                ),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(CELL_ID));
        temporary.entries.push(Entry::Record(reference));
        temporary.entries.push(Entry::Record(disabled_reference));
        temporary
            .entries
            .push(Entry::Record(parent_enabled_reference));
        temporary.entries.push(Entry::Record(ordinary_reference));
        let mut children = Group::new(GroupKind::CellChildren(CELL_ID));
        children.entries.push(Entry::Group(temporary));
        let mut sub = Group::new(GroupKind::ExteriorSubBlock {
            y: 0,
            x: CELL_X as i16,
        });
        sub.entries.push(Entry::Record(cell));
        sub.entries.push(Entry::Group(children));
        let mut block = Group::new(GroupKind::ExteriorBlock {
            y: 0,
            x: CELL_X as i16,
        });
        block.entries.push(Entry::Group(sub));
        let mut world_children = Group::new(GroupKind::WorldChildren(WORLD_ID));
        world_children.entries.push(Entry::Group(block));
        let mut worlds = Group::new(GroupKind::Top(tag::WRLD));
        worlds.entries.push(Entry::Record(world));
        worlds.entries.push(Entry::Group(world_children));
        plugin.groups.extend([statics, worlds]);
        std::fs::write(directory.join("Skyrim.esm"), plugin.to_bytes()).unwrap();
        for name in [
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ] {
            std::fs::write(
                directory.join(name),
                Plugin::new(FileHeader::default()).to_bytes(),
            )
            .unwrap();
        }
    }

    #[test]
    fn edt_25_bto_consumer_generates_and_records_simplified_placement_geometry() {
        let directory = std::env::temp_dir().join(format!("soe-edt-25-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        write_fixture(&directory);
        let source = DataSource::open(&directory).unwrap();
        let (world, editor_id) = source.resolve_world("Edt25World").unwrap();
        let block = LodBlockCoord {
            level: crate::terrain_lod::LodLevel::L4,
            x: CELL_X,
            y: 0,
        };
        let loaded = source
            .load_object_lod_block(world, &editor_id, block)
            .unwrap()
            .expect("missing BTO falls back to generated placement geometry");
        assert_eq!(loaded.authored_references, 1);
        assert_eq!(loaded.simplified_references, 1);
        assert_eq!(loaded.source_triangles, 2);
        assert_eq!(loaded.triangles, 1);
        assert!(loaded.approx_bytes > 0);
        assert!(loaded.approx_bytes < loaded.source_approx_bytes);
        assert_eq!(loaded.surfaces.len(), 1);
        assert_eq!(
            loaded.surfaces[0].arrays.indices.len() / 3,
            loaded.triangles
        );
        assert_eq!(loaded.surfaces[0].reference, world);
        let positions = &loaded.surfaces[0].arrays.positions;
        let unit = crate::units::METRES_PER_UNIT;
        assert!((positions[1][0] - positions[0][0]).abs() < 1e-5);
        assert!((positions[1][2] - positions[0][2] + 2.0 * unit).abs() < 1e-5);
        assert!((positions[2][0] - positions[0][0] + 2.0 * unit).abs() < 1e-5);
        assert!((positions[2][2] - positions[1][2]).abs() < 1e-5);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn edt_25_simplifier_keeps_all_referenced_vertex_attributes_aligned() {
        let file = soe_nif::parse(&source_mesh()).unwrap();
        let mesh = file.meshes().unwrap().remove(0);
        let reduced = simplify_mesh(&mesh, 2);
        assert_eq!(reduced.indices.len(), 3);
        assert_eq!(reduced.positions.len(), 3);
        assert_eq!(reduced.uvs.len(), 3);
        assert_eq!(reduced.normals.len(), 3);
    }
}
