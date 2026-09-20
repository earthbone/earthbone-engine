// SPDX-License-Identifier: GPL-3.0-or-later

//! A real Skyrim SE `Data` folder as a source of cells and meshes: the
//! `esm`, `bsa` and `nif` crates wired together and fed into `mesh::build_surface`.
//!
//! `DataSource::open` loads the five base masters through a `LoadOrder`,
//! builds the `CellIndex` over them, and builds a `bsa::Vfs` from their
//! associated archives plus the data folder as a loose overlay.
//! `DataSource::stream_cell` resolves a cell by editor id through the index,
//! walks the `REFR` ids the index files under it, and for each STAT or furniture base
//! with a model, reads the NIF through the `Vfs`, parses it, places every
//! shape at the reference's own position and rotation, and runs it through
//! the same `mesh::build_surface` the synthetic path already used. Nothing
//! here touches Godot, so it builds and tests without the `godot` feature.
//!
//! `stream_cell` also resolves each surface's material: `MaterialDesc`'s
//! diffuse and normal texture paths are Windows style and case insensitive,
//! same as a NIF's mesh path, so they go through the same `Vfs::read`, which
//! normalises separators and case before it looks the path up (loose files
//! win over archives there, same as meshes). A block-compressed file's
//! payload and mip chain pass through untouched as `TextureData::Compressed`
//! for Godot to upload as is, anything else decodes its top mip to RGBA8.
//! The read runs on the same worker threads that parse the NIF, into a
//! `TextureCache` the `DataSource` owns, so a texture shared by many statics
//! across many cells is read once for the source's lifetime. The trace of
//! 2026-09-18 had a per-call cache decoding every texture on the calling
//! thread after the workers joined, at 97 percent of the stream time.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use soe_audio::{VoiceArchiveError, VoiceLocale};
use soe_bsa::{Archive, Vfs};
use soe_esm::records::refr::Refr;
use soe_esm::{tag, CellIndex, GlobalFormId, LoadOrder};
use soe_nif::{Mat33, Mesh as NifMesh, NifFile, Transform as NifTransform, Vec3 as NifVec3};

use crate::mesh::{
    build_surface, AlphaMode, MaterialDesc, MeshData, MeshError, MeshOptions, SurfaceArrays,
};

mod exterior;
mod location;
mod models;
mod textures;
mod workers;

#[cfg(test)]
mod marker_survey_tests;

pub use location::LoadedLocation;
#[cfg(feature = "godot")]
pub(crate) use location::PreparedCell;
pub(crate) use models::{is_editor_marker_name, model_for};
use textures::TextureCache;
pub use textures::{BlockFormat, TextureData};
use workers::ordered_map;

/// The base masters every Skyrim SE install ships, in the order the game
/// itself loads them.
const BASE_MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

#[derive(Debug, thiserror::Error)]
pub enum DataSourceError {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{name}: {source}")]
    Plugin {
        name: String,
        source: soe_esm::Error,
    },
    #[error("{0}")]
    Esm(#[from] soe_esm::Error),
    #[error("{0}")]
    Bsa(#[from] soe_bsa::Error),
    #[error("{0}")]
    Nif(#[from] soe_nif::Error),
    #[error("{0}")]
    Mesh(#[from] MeshError),
    #[error("{0}")]
    Dds(#[from] soe_dds::Error),
    #[error("{0}")]
    Profile(#[from] soe_profile::Error),
    #[error("{0}")]
    VoiceLocale(#[from] VoiceArchiveError),
    #[error("{0} does not name a game Data directory")]
    DataDirectory(PathBuf),
    #[error("the selected voice archive {0} is unavailable")]
    MissingVoiceArchive(String),
    #[error("no voice archive is available for the selected language")]
    NoVoiceArchives,
    #[error("cell '{0}' was not found in the load order")]
    CellNotFound(String),
    #[error("a data source worker panicked")]
    WorkerPanicked,
    #[error("collision: {0}")]
    Collision(String),
}

/// Counts from one `stream_cell` run, for the headless smoke test and the
/// gated real-data test to report. Not `Copy`, `textures_missing` is a list.
#[derive(Debug, Clone, Default)]
pub struct CellStats {
    pub references: usize,
    pub statics_with_model: usize,
    pub furniture_with_model: usize,
    pub containers_with_model: usize,
    pub doors_with_model: usize,
    pub editor_markers_filtered: usize,
    pub editor_marker_only_models: usize,
    pub meshes_parsed: usize,
    pub meshes_failed: usize,
    pub triangles: usize,
    /// Distinct diffuse, normal, glow, cubemap and mask path sets in the cell.
    /// GPU material variants can also differ by flags and authored scalar inputs.
    pub distinct_texture_sets: usize,
    /// Texture files decoded, counted once per distinct lowercase path, not
    /// once per surface that names it.
    pub textures_decoded: usize,
    /// Texture paths a material named that were not found through the `Vfs`,
    /// lowercased and deduplicated. The caller reports the first few.
    pub textures_missing: Vec<String>,
    /// Texture paths that were found but failed to parse or decode as DDS.
    pub texture_decode_failures: usize,
    /// Time spent inside `Vfs::read` and, for formats Godot cannot take,
    /// `Dds::decode` for every texture actually read during this call, cache
    /// hits excluded, summed across the worker threads, so it can exceed the
    /// call's wall time and it includes time a worker spent waiting on the
    /// archive behind another worker's read. Archive indexing and load order
    /// setup are outside this, they are counted once in the surrounding
    /// `stream_cell_id` wall time the caller times itself.
    pub texture_decode_time: std::time::Duration,
}

/// One built surface with its material's textures decoded to RGBA8, ready
/// for the Godot layer to upload. Kept apart from `SurfaceArrays`, which the
/// synthetic streaming path also builds and has no `Vfs` to read textures
/// from.
#[derive(Clone)]
pub struct LoadedSurface {
    /// The `REFR` that placed this surface, or the `LAND` record for a
    /// terrain surface, so a batched triangle can be traced back to a form.
    pub reference: GlobalFormId,
    pub arrays: SurfaceArrays,
    pub textures: SurfaceTextures,
}

/// A surface's 2D textures and environment cubemap, ready to upload. `None` means
/// the material named no texture in that slot, or the read or decode
/// failed, counted in `CellStats` either way. `Arc` because a texture shared
/// by many statics is read once on whichever worker thread reaches it first
/// and cloned out of `TextureCache` cheaply for every surface that uses it.
#[derive(Debug, Clone, Default)]
pub struct SurfaceTextures {
    pub environment: Option<Arc<TextureData>>,
    pub environment_mask: Option<Arc<TextureData>>,
    pub diffuse: Option<Arc<TextureData>>,
    /// Skyrim packs specular into this texture's alpha channel. The RGB
    /// channels alone are the normal map, alpha is not a transparency mask
    /// here even though the decoded form carries one.
    pub normal: Option<Arc<TextureData>>,
    pub glow: Option<Arc<TextureData>>,
}

pub struct LoadedCell {
    pub stats: CellStats,
    pub surfaces: Vec<LoadedSurface>,
}

/// One exterior cell of a block, by grid position in its worldspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExteriorCell {
    pub x: i32,
    pub y: i32,
    pub id: GlobalFormId,
}

/// The exterior cells `DataSource::resolve_exterior_block` found around a
/// named cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExteriorBlock {
    pub world: GlobalFormId,
    pub world_editor_id: Option<String>,
    pub origin: (i32, i32),
    pub radius: i32,
    pub cells: Vec<ExteriorCell>,
    /// The parent worldspace whose `LAND` fills cells without their own,
    /// when the worldspace's `PNAM` says to use the parent's land data.
    pub land_parent: Option<GlobalFormId>,
    /// The parent's cells at the block's grid positions.
    pub parent_cells: Vec<ExteriorCell>,
}

pub struct LoadedExterior {
    pub block: ExteriorBlock,
    /// In the block's order, each cell with its terrain appended as the
    /// last surface when a `LAND` record exists for it.
    pub cells: Vec<(ExteriorCell, LoadedCell)>,
    pub land_cells: usize,
}

impl LoadedExterior {
    pub fn references(&self) -> usize {
        self.cells.iter().map(|(_, c)| c.stats.references).sum()
    }

    pub fn meshes_parsed(&self) -> usize {
        self.cells.iter().map(|(_, c)| c.stats.meshes_parsed).sum()
    }

    pub fn meshes_failed(&self) -> usize {
        self.cells.iter().map(|(_, c)| c.stats.meshes_failed).sum()
    }

    pub fn triangles(&self) -> usize {
        self.cells.iter().map(|(_, c)| c.stats.triangles).sum()
    }

    pub fn surfaces(&self) -> usize {
        self.cells.iter().map(|(_, c)| c.surfaces.len()).sum()
    }
}

/// An opened `Data` folder: the load order and the archive and loose file
/// view built from it.
pub struct DataSource {
    order: std::sync::Arc<LoadOrder>,
    vfs: Vfs,
    threads: usize,
    textures: TextureCache,
}

/// Parse the base masters, on `threads` workers when more than one, and hand
/// them back in master order so the load order stays deterministic.
fn parse_masters(data_dir: &Path, threads: usize) -> Result<Vec<soe_esm::Plugin>, DataSourceError> {
    let _span = soe_trace::Span::with_payload("host.parse_masters", "host", threads as u64);
    let parse_one = |name: &str| -> Result<soe_esm::Plugin, DataSourceError> {
        let path = data_dir.join(name);
        soe_esm::read_plugin(&path)
            .map_err(|source| DataSourceError::Io {
                path: path.clone(),
                source,
            })?
            .map_err(|source| DataSourceError::Plugin {
                name: name.to_string(),
                source,
            })
    };
    ordered_map(&BASE_MASTERS, threads, |name| parse_one(name))?
        .into_iter()
        .collect()
}

impl DataSource {
    /// Loads a vanilla game's resolved profile from its `Data` directory.
    /// The game's default INI supplies the locale when no user INI is given.
    /// Parsing and mesh building use the machine's parallelism.
    pub fn open(data_dir: &Path) -> Result<Self, DataSourceError> {
        let game_dir = data_dir
            .parent()
            .ok_or_else(|| DataSourceError::DataDirectory(data_dir.to_path_buf()))?;
        let loadout = soe_profile::LoadoutInput {
            game_dir: game_dir.to_path_buf(),
            data_roots: vec![data_dir.to_path_buf()],
            plugins_txt: None,
            loadorder_txt: None,
            ini_dir: None,
            ccc: None,
        }
        .resolve()?;
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self::open_loadout_with_threads(&loadout, threads)
    }

    /// Opens a resolved profile, retaining its archive and loose-root order.
    pub fn open_loadout(loadout: &soe_profile::Loadout) -> Result<Self, DataSourceError> {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self::open_loadout_with_threads(loadout, threads)
    }

    /// `open_loadout` with a fixed worker count, one for the sequential path.
    pub fn open_with_threads(data_dir: &Path, threads: usize) -> Result<Self, DataSourceError> {
        let game_dir = data_dir
            .parent()
            .ok_or_else(|| DataSourceError::DataDirectory(data_dir.to_path_buf()))?;
        let loadout = soe_profile::LoadoutInput {
            game_dir: game_dir.to_path_buf(),
            data_roots: vec![data_dir.to_path_buf()],
            plugins_txt: None,
            loadorder_txt: None,
            ini_dir: None,
            ccc: None,
        }
        .resolve()?;
        Self::open_loadout_with_threads(&loadout, threads)
    }

    /// `open_loadout` with a fixed worker count, one for the sequential path.
    pub fn open_loadout_with_threads(
        loadout: &soe_profile::Loadout,
        threads: usize,
    ) -> Result<Self, DataSourceError> {
        let _span = soe_trace::Span::with_payload("host.open", "host", threads as u64);
        let threads = threads.max(1);
        let data_dir = loadout
            .data_roots
            .first()
            .ok_or_else(|| DataSourceError::DataDirectory(PathBuf::new()))?;
        let parsed = parse_masters(data_dir, threads)?;
        let mut order = LoadOrder::new();
        for (name, plugin) in BASE_MASTERS.iter().zip(parsed) {
            order
                .add(name, plugin)
                .map_err(|source| DataSourceError::Plugin {
                    name: (*name).to_string(),
                    source,
                })?;
        }
        order.build_cell_index();
        let mut vfs = Vfs::new();
        {
            let _archives = soe_trace::Span::new("host.index_archives", "host");
            let paths = archive_paths(loadout, false)?;
            for archive in ordered_map(&paths, threads, |path| Archive::open(path))? {
                vfs.add_archive(archive?);
            }
            for root in &loadout.data_roots {
                vfs.add_loose_root(root)?;
            }
        }
        Ok(DataSource {
            order: std::sync::Arc::new(order),
            vfs,
            threads,
            textures: TextureCache::default(),
        })
    }

    /// The load order behind this source, for a sibling module that walks the
    /// same references, such as [`crate::collision`].
    pub(crate) fn order(&self) -> &LoadOrder {
        &self.order
    }

    /// The archive and loose file view, same reason.
    pub(crate) fn vfs(&self) -> &Vfs {
        &self.vfs
    }

    /// The cell index built at open, for callers that walk cells by grid
    /// position or need a cell's land and navmeshes.
    #[must_use]
    pub fn cell_index(&self) -> &CellIndex {
        self.order.build_cell_index()
    }

    /// Share records and the cached cell index with the authoritative game runtime.
    pub fn shared_order(&self) -> std::sync::Arc<LoadOrder> {
        self.order.clone()
    }

    /// The winning `CELL` record whose editor id matches, preferring an
    /// interior cell when both an interior and an exterior one share a name.
    #[must_use]
    pub fn resolve_cell(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.cell_index().cell_by_editor_id(editor_id)
    }

    /// The first interior cell, by form id, whose reference count exceeds
    /// `min_refs`. Used when a named cell such as `WhiterunBanneredMare` is
    /// missing from an install, so the real-data test still has a populated
    /// interior to load.
    #[must_use]
    pub fn first_populated_interior_cell(&self, min_refs: usize) -> Option<GlobalFormId> {
        let mut candidates: Vec<GlobalFormId> = self
            .cell_index()
            .cells()
            .filter(|&(id, n)| n > min_refs && self.cell_index().is_interior(id))
            .map(|(id, _)| id)
            .collect();
        candidates.sort_unstable();
        candidates.into_iter().next()
    }

    /// Resolves `editor_id` and streams it. See `stream_cell_id`.
    pub fn stream_cell(&self, editor_id: &str) -> Result<LoadedCell, DataSourceError> {
        let cell_id = self
            .resolve_cell(editor_id)
            .ok_or_else(|| DataSourceError::CellNotFound(editor_id.to_string()))?;
        self.stream_cell_id(cell_id)
    }

    /// Worker threads for NIF parsing and mesh building. One means the
    /// sequential path. Defaults to the machine's parallelism.
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads.max(1);
    }

    /// Drop prepared texture entries so the next stream measures cold cache work.
    /// Existing returned textures remain valid through their shared ownership.
    pub fn clear_prepared_textures(&self) {
        self.textures.clear_prepared();
    }

    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads
    }

    /// Walks every `REFR` the index files under `cell_id`, builds a
    /// `SurfaceArrays` for each static, furniture or container model, and reports
    /// what it found. `CellStats::references` counts `REFR` only, actors and
    /// projectiles the index also holds are skipped here.
    /// A reference whose mesh fails to read or parse is counted as failed and
    /// skipped, it does not stop the rest of the cell from loading.
    pub fn stream_cell_id(&self, cell_id: GlobalFormId) -> Result<LoadedCell, DataSourceError> {
        let _span = soe_trace::Span::new("host.stream_cell", "host");
        let mut stats = CellStats::default();
        let work = {
            let _scan = soe_trace::Span::with_payload(
                "host.scan_references",
                "host",
                self.cell_index().references(cell_id).len() as u64,
            );
            let mut work: Vec<(String, Refr, GlobalFormId)> = Vec::new();
            // A placed object whose winner is deleted is not in this slice,
            // the index files it under `deleted_references` instead. It stays
            // resolvable through the load order and the game starts it
            // disabled, so there is nothing here to draw.
            for id in self.cell_index().references(cell_id).iter().copied() {
                let Some(rref) = self.order.winner_ref(id) else {
                    continue;
                };
                let rec = self.order.record(rref);
                if rec.tag != tag::REFR {
                    continue;
                }
                let Ok(refr) = Refr::from_record(rec) else {
                    continue;
                };
                let _reference = soe_trace::Span::new("host.reference", "host");
                stats.references += 1;

                let base_id = self.order.resolve(rref.file, refr.base);
                let Some(base_rec) = self.order.winner(base_id) else {
                    continue;
                };
                let Some(model) = model_for(base_rec) else {
                    continue;
                };
                if base_rec.tag == tag::STAT {
                    stats.statics_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"FURN") {
                    stats.furniture_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"DOOR") {
                    stats.doors_with_model += 1;
                } else {
                    stats.containers_with_model += 1;
                }
                work.push((model, refr, id));
            }
            work
        };

        let before = self.textures.counters();
        let results = self.build_all(&work)?;
        let after = self.textures.counters();
        let mut surfaces = Vec::new();
        let mut texture_sets = HashSet::new();
        for ((model, _, reference), result) in work.iter().zip(results) {
            match result {
                Ok(LoadedStatic::Surfaces {
                    built,
                    marker_shapes_filtered,
                    marker_only,
                }) => {
                    stats.editor_markers_filtered += marker_shapes_filtered;
                    stats.editor_marker_only_models += usize::from(marker_only);
                    stats.meshes_parsed += 1;
                    for (arrays, textures) in built {
                        stats.triangles += arrays.triangle_count();
                        texture_sets.insert((
                            arrays.material.diffuse.as_deref().map(soe_bsa::normalize),
                            arrays.material.normal.as_deref().map(soe_bsa::normalize),
                            arrays.material.glow_texture().map(soe_bsa::normalize),
                            arrays
                                .material
                                .environment_texture()
                                .map(soe_bsa::normalize),
                            arrays
                                .material
                                .environment_mask
                                .as_deref()
                                .map(soe_bsa::normalize),
                        ));
                        surfaces.push(LoadedSurface {
                            reference: *reference,
                            arrays,
                            textures,
                        });
                    }
                }
                Err(e) => {
                    eprintln!("stream_cell: {model}: {e}");
                    stats.meshes_failed += 1;
                }
            }
        }
        stats.distinct_texture_sets = texture_sets.len();
        stats.textures_decoded = after.decoded - before.decoded;
        stats.textures_missing = self.textures.missing_since(before.missing);
        stats.texture_decode_failures = after.failures - before.failures;
        stats.texture_decode_time =
            std::time::Duration::from_nanos(after.decode_ns - before.decode_ns);
        Ok(LoadedCell { stats, surfaces })
    }

    /// Parse and build every model, in order, on `threads` workers over
    /// chunks of the work list, textures included. The archive view and the
    /// texture cache are shared read-only and under a mutex respectively.
    fn build_all(
        &self,
        work: &[(String, Refr, GlobalFormId)],
    ) -> Result<Vec<BuiltStatic>, DataSourceError> {
        let _span = soe_trace::Span::with_payload("host.build_all", "host", work.len() as u64);
        ordered_map(work, self.threads, |(model, refr, _)| {
            self.load_static_surfaces(model, refr)
        })
    }

    fn load_static_surfaces(&self, model: &str, refr: &Refr) -> BuiltStatic {
        let _span = soe_trace::Span::new("host.static", "host");
        let path = format!("meshes/{model}");
        let bytes = self.vfs.read(&path)?;
        let file = placed_model(&bytes)?;
        let marker_blocks = editor_marker_shape_blocks(&file);
        let mut meshes = file.meshes()?;
        let marker_shapes_filtered = meshes
            .iter()
            .filter(|mesh| marker_blocks.contains(&mesh.block))
            .count();
        meshes.retain(|mesh| !marker_blocks.contains(&mesh.block));
        let marker_only = marker_shapes_filtered > 0 && meshes.is_empty();
        let refr_transform = placement_transform(refr);
        let mut out = Vec::with_capacity(meshes.len());
        for shape in &meshes {
            let arrays = {
                let _surface = soe_trace::Span::new("host.build_surface", "host");
                let combined = refr_transform.then(&shape.transform);
                let data = mesh_data_from(shape, &combined);
                build_surface(&data, MeshOptions::default())?
            };
            let textures = {
                let _material = soe_trace::Span::new("host.material", "host");
                self.textures.resolve(&self.vfs, &arrays.material)
            };
            out.push((arrays, textures));
        }
        Ok(LoadedStatic::Surfaces {
            built: out,
            marker_shapes_filtered,
            marker_only,
        })
    }
}

/// Builds the archive and loose-file view for one resolved profile.
pub fn vfs_from_loadout(loadout: &soe_profile::Loadout) -> Result<Vfs, DataSourceError> {
    let mut vfs = Vfs::new();
    for archive in archive_paths(loadout, true)? {
        vfs.add_archive_path(&archive)?;
    }
    for root in &loadout.data_roots {
        vfs.add_loose_root(root)?;
    }
    Ok(vfs)
}

fn archive_paths(
    loadout: &soe_profile::Loadout,
    require_voice_archive: bool,
) -> Result<Vec<PathBuf>, DataSourceError> {
    let locale = match VoiceLocale::from_language(loadout.settings.get("General", "sLanguage")) {
        Ok(locale) => Some(locale),
        Err(error) if require_voice_archive => return Err(error.into()),
        Err(_) => None,
    };
    let mut paths = Vec::new();
    let mut voices = 0;
    for archive in &loadout.archives {
        if VoiceLocale::is_voice_archive(&archive.name) {
            if !locale
                .as_ref()
                .is_some_and(|locale| locale.matches_archive(&archive.name))
            {
                continue;
            }
            let path = archive
                .path
                .clone()
                .ok_or_else(|| DataSourceError::MissingVoiceArchive(archive.name.clone()))?;
            paths.push(path);
            voices += 1;
        } else if let Some(path) = &archive.path {
            paths.push(path.clone());
        }
    }
    if require_voice_archive && voices == 0 {
        return Err(DataSourceError::NoVoiceArchives);
    }
    Ok(paths)
}

/// One static's surfaces with their textures, or why it failed.
type BuiltStatic = Result<LoadedStatic, DataSourceError>;

enum LoadedStatic {
    Surfaces {
        built: Vec<(SurfaceArrays, SurfaceTextures)>,
        marker_shapes_filtered: usize,
        marker_only: bool,
    },
}

fn editor_marker_shape_blocks(file: &NifFile) -> HashSet<usize> {
    editor_marker_shape_indices(&file.blocks)
}

fn editor_marker_shape_indices(blocks: &[soe_nif::blocks::Block]) -> HashSet<usize> {
    let mut marked = HashSet::new();
    for (index, block) in blocks.iter().enumerate() {
        match block {
            soe_nif::blocks::Block::Node(node) if is_editor_marker_name(node.av.name()) => {
                mark_descendant_shapes(blocks, index, &mut marked);
            }
            soe_nif::blocks::Block::TriShape(shape) if is_editor_marker_name(shape.av.name()) => {
                marked.insert(index);
            }
            soe_nif::blocks::Block::BsTriShape(shape) if is_editor_marker_name(shape.av.name()) => {
                marked.insert(index);
            }
            _ => {}
        }
    }
    marked
}

fn mark_descendant_shapes(
    blocks: &[soe_nif::blocks::Block],
    index: usize,
    marked: &mut HashSet<usize>,
) {
    let mut pending = vec![index];
    let mut visited = HashSet::new();
    while let Some(index) = pending.pop() {
        if !visited.insert(index) {
            continue;
        }
        match blocks.get(index) {
            Some(soe_nif::blocks::Block::Node(node)) => {
                pending.extend(node.children.iter().filter_map(|child| child.get()));
            }
            Some(soe_nif::blocks::Block::TriShape(_) | soe_nif::blocks::Block::BsTriShape(_)) => {
                marked.insert(index);
            }
            _ => {}
        }
    }
}

/// A placed model replaces its single root node's export transform with REFR
/// placement. OpenMW's NiNode::read applies
/// this rule to block-zero roots except Bip01. Keep it at the host boundary so
/// raw NIF inspection and actor bind transforms retain the original data.
pub(crate) fn placed_model(bytes: &[u8]) -> Result<soe_nif::NifFile, DataSourceError> {
    use soe_nif::blocks::Block;
    let mut file = soe_nif::parse(bytes)?;
    // nifly reads Matrix3 directly into rows and applies row-dot-vector.
    // Our raw reader follows nif.xml's column naming, so transpose each local
    // scene rotation before composition. Translation and scale stay authored.
    for block in &mut file.blocks {
        let av = match block {
            Block::Node(node) => &mut node.av,
            Block::TriShape(shape) => &mut shape.av,
            Block::BsTriShape(shape) => &mut shape.av,
            _ => continue,
        };
        let rows = av.transform.rotation.rows;
        av.transform.rotation.rows = std::array::from_fn(|r| std::array::from_fn(|c| rows[c][r]));
    }
    if file.roots.as_slice() == [soe_nif::BlockRef(0)] {
        if let Some(soe_nif::blocks::Block::Node(node)) = file.blocks.first_mut() {
            if !node.av.name().eq_ignore_ascii_case("bip01") {
                node.av.transform = NifTransform::IDENTITY;
            }
        }
    }
    Ok(file)
}

/// Builds the reference's own placement transform from its `DATA` position
/// and Euler rotation. The rotation matrix matches
/// `NiMatrix3::SetEulerAnglesXYZ` in CommonLibSSE, which is how the engine
/// turns a reference's rotation floats into its world orientation.
fn placement_transform(refr: &Refr) -> NifTransform {
    let t = refr.transform.unwrap_or_default();
    let [rx, ry, rz] = t.rotation;
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let rotation = Mat33 {
        rows: [
            [cy * cz, cy * sz, -sy],
            [sx * sy * cz - cx * sz, sx * sy * sz + cx * cz, sx * cy],
            [cx * sy * cz + sx * sz, cx * sy * sz - sx * cz, cx * cy],
        ],
    };
    NifTransform {
        rotation,
        translation: NifVec3::new(t.position[0], t.position[1], t.position[2]),
        scale: if refr.scale == 0.0 { 1.0 } else { refr.scale },
    }
}

/// The same placement as [`placement_transform`], in the physics crate's
/// transform type. Both matrices are row major and apply the same way, and
/// both carry a uniform scale, so this is a field copy rather than a
/// conversion. Kept next to its twin so the two cannot drift.
pub(crate) fn placement_iso(refr: &Refr) -> soe_physics::Iso {
    let t = placement_transform(refr);
    soe_physics::Iso {
        rotation: soe_physics::Mat3 {
            rows: t.rotation.rows,
        },
        translation: soe_physics::Vec3::new(t.translation.x, t.translation.y, t.translation.z),
        scale: t.scale,
    }
}

/// Bakes `combined` (the reference's placement composed with the shape's own
/// transform inside its NIF) into the shape's vertex data and adapts the
/// `nif` crate's neutral mesh to godot-host's `MeshSource` shape. Bone
/// indices widen from the file's u8 to the four-bone `u16` layout `mesh::MeshData`
/// carries, since a skeleton can exceed 256 bones.
pub(crate) fn mesh_data_from(shape: &NifMesh, combined: &NifTransform) -> MeshData {
    let positions = shape
        .positions
        .iter()
        .map(|&[x, y, z]| combined.apply(NifVec3::new(x, y, z)).to_array())
        .collect();
    let normals = (!shape.normals.is_empty()).then(|| {
        shape
            .normals
            .iter()
            .map(|&[x, y, z]| combined.apply_direction(NifVec3::new(x, y, z)).to_array())
            .collect()
    });
    let tangents = (!shape.tangents.is_empty()).then(|| {
        shape
            .tangents
            .iter()
            .map(|&[x, y, z, w]| {
                let v = combined.apply_direction(NifVec3::new(x, y, z));
                [v.x, v.y, v.z, w]
            })
            .collect()
    });
    let uvs = (!shape.uvs.is_empty()).then(|| shape.uvs.clone());
    let bone_indices = (!shape.bone_indices.is_empty()).then(|| {
        shape
            .bone_indices
            .iter()
            .map(|b| {
                [
                    u16::from(b[0]),
                    u16::from(b[1]),
                    u16::from(b[2]),
                    u16::from(b[3]),
                ]
            })
            .collect()
    });
    let bone_weights = (!shape.bone_weights.is_empty()).then(|| shape.bone_weights.clone());
    MeshData {
        positions,
        normals,
        tangents,
        uvs,
        indices: shape.indices.clone(),
        bone_indices,
        bone_weights,
        material: material_desc_from(shape),
    }
}

fn material_desc_from(shape: &NifMesh) -> MaterialDesc {
    let m = &shape.material;
    let alpha = if m.blend.is_some() {
        AlphaMode::Blend
    } else if let Some(test) = &m.alpha_test {
        AlphaMode::Cutout((test.threshold * 255.0).round().clamp(0.0, 255.0) as u8)
    } else {
        AlphaMode::Opaque
    };
    MaterialDesc {
        diffuse: m.diffuse().map(str::to_string),
        normal: m.normal().map(str::to_string),
        glow: m.glow().map(str::to_string),
        emissive_color: m.emissive_color,
        emissive_multiple: m.emissive_multiple,
        environment: m.environment().map(str::to_string),
        environment_mask: m.environment_mask().map(str::to_string),
        environment_scale: match m.extra {
            soe_nif::blocks::ShaderExtra::EnvironmentMapScale(scale) => scale,
            soe_nif::blocks::ShaderExtra::MultiLayerParallax {
                envmap_strength, ..
            } => envmap_strength,
            _ => 1.0,
        },
        shader_flags_1: m.flags1,
        shader_flags_2: m.flags2,
        alpha,
        double_sided: m.double_sided,
    }
}

#[cfg(test)]
mod placement_tests;

#[cfg(test)]
mod emission_tests;

#[cfg(test)]
#[path = "../../nif/tests/common/mod.rs"]
mod fixture;

#[cfg(test)]
mod environment_tests;

#[cfg(test)]
mod marker_shape_tests;
