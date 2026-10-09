// SPDX-License-Identifier: GPL-3.0-or-later

//! A real Skyrim SE `Data` folder as a source of cells and meshes: the
//! `esm`, `bsa` and `nif` crates wired together and fed into `mesh::build_surface`.
//!
//! `DataSource::open` loads the five base masters through a `LoadOrder`,
//! builds the `CellIndex` over them, and builds a `bsa::Vfs` from their
//! associated archives plus the data folder as a loose overlay.
//! `DataSource::stream_cell` resolves a cell by editor id through the index,
//! walks the `REFR` ids the index files under it, and for each placed object base
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
//!
//! Skinned character surfaces resolve their material textures through the
//! same DDS loader, using the character archives supplied to their VFS.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use soe_audio::{VoiceArchiveError, VoiceLocale};
use soe_bsa::{Archive, Vfs};
use soe_esm::records::cell::Cell;
use soe_esm::records::refr::Refr;
use soe_esm::{tag, CellIndex, GlobalFormId, LoadOrder};
#[cfg(any(feature = "godot", test))]
use soe_esm::{Record, Tag};
use soe_nif::{Mat33, Mesh as NifMesh, NifFile, Transform as NifTransform, Vec3 as NifVec3};

use crate::mesh::{
    build_surface, AlphaMode, MaterialDesc, MeshData, MeshError, MeshOptions, SurfaceArrays,
    WaterMaterial,
};

#[cfg(feature = "godot")]
mod chargen;
mod exterior;
mod grass;
mod location;
mod lod;
mod models;
mod reference_audit;
mod terrain_texture;
mod textures;
mod water;
mod workers;

#[cfg(test)]
mod destruction_tests;
#[cfg(test)]
mod marker_survey_tests;
#[cfg(test)]
mod prf_20_tests;
#[cfg(test)]
mod rnd_20_tests;
#[cfg(test)]
mod rnd_32_tests;
#[cfg(test)]
mod rnd_33_tests;
#[cfg(test)]
mod ui_25_tests;

#[cfg(feature = "godot")]
pub(crate) use chargen::ChargenCache;
pub use location::LoadedLocation;
#[cfg(feature = "godot")]
pub(crate) use location::PreparedCell;
pub use lod::LoadedLodBlock;
pub(crate) use models::{is_editor_marker_name, model_for};
pub use reference_audit::ReferenceCounts;
pub(crate) use textures::checker_albedo;
use textures::TextureCache;
pub use textures::{BlockFormat, TextureData};
use water::water_material_from;
use workers::WorkerPools;

/// Resolve material textures through the same VFS path as streamed meshes.
pub(crate) fn resolve_material_textures(
    vfs: &Vfs,
    materials: &[MaterialDesc],
) -> Vec<SurfaceTextures> {
    let cache = TextureCache::default();
    materials
        .iter()
        .map(|material| cache.resolve(vfs, material))
        .collect()
}

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
    #[error("localized text: {0}")]
    Localization(String),
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
    #[error("radius {0} is negative, an exterior block needs a radius of 0 or more")]
    InvalidRadius(i32),
    #[error("a data source worker panicked")]
    WorkerPanicked,
    #[error("building a data source worker pool: {0}")]
    WorkerPoolBuild(#[source] rayon::ThreadPoolBuildError),
    #[error("collision: {0}")]
    Collision(String),
}

/// Counts from one `stream_cell` run, for the headless smoke test and the
/// gated real-data test to report. Not `Copy`, `textures_missing` is a list.
#[derive(Debug, Clone, Default)]
pub struct CellStats {
    /// Cell-local attribution retained through collision preparation and upload.
    pub profile: soe_trace::LoadRecorder,
    pub references: usize,
    /// Mutually exclusive placed-reference outcomes, ordered by base signature.
    pub reference_counts: std::collections::BTreeMap<soe_esm::Tag, ReferenceCounts>,
    /// Mesh preparation outcomes for references with no returned surface.
    pub reference_skips: std::collections::BTreeMap<GlobalFormId, &'static str>,
    pub statics_with_model: usize,
    pub furniture_with_model: usize,
    pub containers_with_model: usize,
    pub activators_with_model: usize,
    pub doors_with_model: usize,
    /// 1 when the cell has water and its plane was built, 0 otherwise. A
    /// count rather than a bool since every other field here counts, and a
    /// cell has at most one water plane no matter how many WATR-carrying
    /// references or XCWT overrides it has.
    pub water_plane_count: usize,
    /// Grass instances placed from LAND texture layers, see `data_source::grass`.
    /// Not a `REFR` count, grass has none.
    pub grass_placements: usize,
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
    /// The reference's placement at load time, in Skyrim space, already
    /// baked into `arrays` unless `instance_transform` is set, when `arrays`
    /// holds the shape's local geometry instead. A terrain surface carries
    /// identity here since `LAND` has no separate placement. Kept so a
    /// runtime placement change can be expressed as a delta against this
    /// baseline instead of the world-space vertices themselves.
    pub placement: soe_physics::Iso,
    /// The reference's placement in Godot space, set only when
    /// `stream_cell_id` decided this surface's model repeats often enough in
    /// the cell to draw as a `MultiMesh` instead of batching it. When set,
    /// `arrays` holds the shape's local geometry, not baked to this
    /// placement, and `batch::instancing_candidates` is what reads this
    /// field back out.
    pub instance_transform: Option<crate::units::GodotTransform>,
    /// The model path and shape index this surface came from, lowercased,
    /// set together with `instance_transform`. Surfaces that share this
    /// group into one `MultiMesh`.
    pub mesh_identity: Option<Arc<str>>,
}

/// A surface's 2D textures and environment cubemap, ready to upload. A failed
/// named diffuse uses the checker. Other slots remain `None` when absent or
/// unreadable, with failed reads counted in `CellStats`. `Arc` because a texture shared
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
    pub greyscale: Option<Arc<TextureData>>,
}

pub struct LoadedCell {
    pub stats: CellStats,
    pub surfaces: Vec<LoadedSurface>,
}

impl LoadedCell {
    /// Distinct texture payloads referenced by this cell, including generated
    /// terrain and complete cubemap faces. Shared cells each report their usage.
    pub fn texture_inventory(&self) -> std::collections::BTreeMap<String, usize> {
        let mut inventory = std::collections::BTreeMap::new();
        for surface in &self.surfaces {
            let material = &surface.arrays.material;
            let textures = &surface.textures;
            for (path, texture) in [
                (material.diffuse.as_deref(), textures.diffuse.as_deref()),
                (material.normal.as_deref(), textures.normal.as_deref()),
                (material.glow_texture(), textures.glow.as_deref()),
                (
                    material.environment_texture(),
                    textures.environment.as_deref(),
                ),
                (
                    material.environment_mask.as_deref(),
                    textures.environment_mask.as_deref(),
                ),
            ] {
                if let (Some(path), Some(texture)) = (path, texture) {
                    inventory
                        .entry(soe_bsa::normalize(path))
                        .or_insert_with(|| texture.payload_bytes());
                }
            }
        }
        inventory
    }

    /// Refresh after exterior terrain and grass have been appended.
    pub(crate) fn refresh_texture_inventory(&self) {
        self.stats.profile.set_textures(self.texture_inventory());
    }
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
    vfs: std::sync::Arc<Vfs>,
    threads: usize,
    workers: WorkerPools,
    textures: TextureCache,
    profile_settings: soe_profile::Settings,
    profile_ini_dir: Option<PathBuf>,
    language: String,
}

/// A winning cell record prepared for the editor's worldspace and cell tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorCell {
    /// The resolved cell form id, formatted as `file:object`.
    pub id: String,
    /// The cell editor id, or its form id when it has no editor id.
    pub name: String,
    /// The resolved worldspace form id for an exterior cell.
    pub world_id: Option<String>,
    /// The worldspace editor id for an exterior cell.
    pub world_name: Option<String>,
    /// Whether this is an interior cell.
    pub interior: bool,
    /// Exterior grid X coordinate.
    pub x: Option<i32>,
    /// Exterior grid Y coordinate.
    pub y: Option<i32>,
    /// Number of live placed objects indexed under this cell.
    pub references: usize,
}

/// Parse the base masters, on `threads` workers when more than one, and hand
/// them back in master order so the load order stays deterministic.
fn plugin_cache_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SOE_PLUGIN_CACHE") {
        return (path != "off").then(|| PathBuf::from(path));
    }
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(
            PathBuf::from(path)
                .join("skyrim-open-engine")
                .join("plugins"),
        );
    }
    #[cfg(windows)]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Some(
            PathBuf::from(path)
                .join("skyrim-open-engine")
                .join("plugins"),
        );
    }
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".cache")
            .join("skyrim-open-engine")
            .join("plugins")
    })
}

fn parse_masters(
    data_dir: &Path,
    threads: usize,
    workers: &WorkerPools,
    cache_dir: Option<&Path>,
) -> Result<Vec<soe_esm::Plugin>, DataSourceError> {
    let _span = soe_trace::Span::with_payload("host.parse_masters", "host", threads as u64);
    let parse_one = |name: &str| -> Result<soe_esm::Plugin, DataSourceError> {
        let _plugin = soe_trace::LoadScope::object(format!("plugin:{name}"));
        let path = data_dir.join(name);
        let parsed = if let Some(cache_dir) = cache_dir {
            soe_esm::read_plugin_cached(&path, cache_dir)
        } else {
            soe_esm::read_plugin(&path)
        };
        parsed
            .map_err(|source| DataSourceError::Io {
                path: path.clone(),
                source,
            })?
            .map_err(|source| DataSourceError::Plugin {
                name: name.to_string(),
                source,
            })
    };
    workers
        .ordered_map(&BASE_MASTERS, threads, |name| parse_one(name))?
        .into_iter()
        .collect()
}

impl DataSource {
    /// Requested capacity of retained record bodies, grouped by defining plugin.
    /// Includes losing overrides and the raw TES4 record. Load-order indices and
    /// decoded header metadata are outside these record body totals.
    pub fn record_memory_by_plugin(&self) -> std::collections::BTreeMap<String, usize> {
        fn body(record: &soe_esm::Record) -> usize {
            record.subrecords.capacity() * std::mem::size_of::<soe_esm::Subrecord>()
                + record
                    .subrecords
                    .iter()
                    .map(|sub| sub.data.capacity())
                    .sum::<usize>()
        }
        self.order
            .files()
            .iter()
            .map(|file| {
                let plugin = &file.plugin;
                let bytes = plugin.records.capacity() * std::mem::size_of::<soe_esm::Record>()
                    + plugin.records.iter().map(body).sum::<usize>()
                    + std::mem::size_of::<soe_esm::Record>()
                    + body(&plugin.header_record);
                (file.name.clone(), bytes)
            })
            .collect()
    }
    /// Load an EffectShader membrane texture through the source's shared DDS cache.
    #[cfg(feature = "godot")]
    pub(crate) fn effect_shader_texture(&self, path: &str) -> Option<Arc<TextureData>> {
        self.textures.get_or_decode(&self.vfs, path)
    }

    /// Loads a vanilla game's resolved profile from its `Data` directory.
    /// The game's default INI supplies the locale when no user INI is given.
    /// Share archive handles and records, with a private speculative texture cache.
    /// One worker prepares the destination without occupying all available CPUs.
    pub(crate) fn speculative_view(&self) -> Self {
        Self {
            order: std::sync::Arc::clone(&self.order),
            vfs: std::sync::Arc::clone(&self.vfs),
            threads: 1,
            workers: self.workers.clone(),
            textures: TextureCache::default(),
            profile_settings: self.profile_settings.clone(),
            profile_ini_dir: self.profile_ini_dir.clone(),
            language: self.language.clone(),
        }
    }

    /// Loads the base masters and their associated archives from `data_dir`,
    /// then adds `data_dir` itself as a loose overlay so a texture or mesh
    /// replacer placed there beats the archives, matching the shipped engine.
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
        Self::open_with_threads_and_cache(data_dir, threads, plugin_cache_dir().as_deref())
    }

    /// `open_with_threads` with an explicit parsed-plugin cache directory.
    /// Passing `None` disables the parsed-plugin cache.
    pub fn open_with_threads_and_cache(
        data_dir: &Path,
        threads: usize,
        cache_dir: Option<&Path>,
    ) -> Result<Self, DataSourceError> {
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
        Self::open_loadout_with_threads_and_cache(&loadout, threads, cache_dir)
    }

    /// `open_loadout` with a fixed worker count, one for the sequential path.
    pub fn open_loadout_with_threads(
        loadout: &soe_profile::Loadout,
        threads: usize,
    ) -> Result<Self, DataSourceError> {
        Self::open_loadout_with_threads_and_cache(loadout, threads, plugin_cache_dir().as_deref())
    }

    /// `open_loadout_with_threads` with an explicit parsed-plugin cache directory.
    /// Passing `None` disables the parsed-plugin cache.
    pub fn open_loadout_with_threads_and_cache(
        loadout: &soe_profile::Loadout,
        threads: usize,
        cache_dir: Option<&Path>,
    ) -> Result<Self, DataSourceError> {
        let _load = soe_trace::LoadScope::operation(
            "startup",
            "DataSource",
            std::time::Duration::from_secs(18),
        );
        let _span = soe_trace::Span::with_payload("host.open", "host", threads as u64);
        let threads = threads.max(1);
        let workers = WorkerPools::default();
        let data_dir = loadout
            .data_roots
            .first()
            .ok_or_else(|| DataSourceError::DataDirectory(PathBuf::new()))?;
        let parsed = parse_masters(data_dir, threads, &workers, cache_dir)?;
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
            for archive in workers.ordered_map(&paths, threads, |path| Archive::open(path))? {
                vfs.add_archive(archive?);
            }
            for root in &loadout.data_roots {
                vfs.add_loose_root(root)?;
            }
        }
        Ok(DataSource {
            order: std::sync::Arc::new(order),
            vfs: std::sync::Arc::new(vfs),
            threads,
            workers,
            textures: TextureCache::default(),
            profile_settings: loadout.settings.clone(),
            profile_ini_dir: loadout.ini_dir.clone(),
            language: loadout
                .settings
                .get("General", "sLanguage")
                .unwrap_or("english")
                .to_ascii_lowercase(),
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

    /// Create the menu and gameplay world with the profile's localized tables.
    ///
    /// # Errors
    /// Reports unreadable or malformed selected-language string tables.
    pub fn localized_game(&self) -> Result<soe_game::Game, DataSourceError> {
        let game = soe_game::Game::from_shared_order(self.shared_order());
        game.world_mut()
            .load_localized_strings(&self.vfs, &self.language)
            .map_err(|error| DataSourceError::Localization(error.to_string()))?;
        Ok(game)
    }

    /// Build an inventory ground model at the origin through the world mesh pipeline.
    ///
    /// # Errors
    /// Reports missing assets, malformed NIFs, or invalid surfaces.
    #[cfg(any(feature = "godot", test))]
    pub(crate) fn item_preview_surfaces(
        &self,
        model: &str,
    ) -> Result<Vec<(SurfaceArrays, SurfaceTextures)>, DataSourceError> {
        let reference = Refr::from_record(
            &Record::new(tag::REFR, 0).sub(Tag::new(b"NAME"), 0_u32.to_le_bytes()),
        )?;
        let LoadedStatic::Surfaces { built, .. } =
            self.load_static_surfaces(model, &reference, false)?;
        Ok(built)
    }

    /// The cell index built at open, for callers that walk cells by grid
    /// position or need a cell's land and navmeshes.
    #[must_use]
    pub fn cell_index(&self) -> &CellIndex {
        self.order.build_cell_index()
    }

    /// Returns winning cells in a stable order for an editor tree.
    /// Exterior cells carry their worldspace and grid coordinates. Interior
    /// cells have no worldspace parent in the plugin record hierarchy.
    pub fn editor_cells(&self) -> Vec<EditorCell> {
        let index = self.cell_index();
        let mut cells = Vec::new();
        for (id, references) in index.cells() {
            let Some(record) = self.order.winner(id) else {
                continue;
            };
            if record.tag != tag::CELL {
                continue;
            }
            let grid = index.grid_of(id);
            let world_id = grid.map(|grid| grid.world);
            let world_name = world_id.and_then(|world| {
                self.order
                    .winner(world)
                    .and_then(|record| record.editor_id())
            });
            cells.push(EditorCell {
                id: id.to_string(),
                name: record.editor_id().unwrap_or_else(|| id.to_string()),
                world_id: world_id.map(|world| world.to_string()),
                world_name,
                interior: index.is_interior(id),
                x: grid.map(|grid| grid.x),
                y: grid.map(|grid| grid.y),
                references,
            });
        }
        cells.sort_by(|left, right| {
            (left.interior, &left.world_id, left.y, left.x, &left.id).cmp(&(
                right.interior,
                &right.world_id,
                right.y,
                right.x,
                &right.id,
            ))
        });
        cells
    }

    /// Share records and the cached cell index with the authoritative game runtime.
    pub fn shared_order(&self) -> std::sync::Arc<LoadOrder> {
        self.order.clone()
    }

    /// Resolved profile settings and INI directory for runtime diagnostics.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    pub fn profile_papyrus_logs(&self) -> (&soe_profile::Settings, Option<&Path>) {
        (&self.profile_settings, self.profile_ini_dir.as_deref())
    }

    /// The winning `CELL` record whose editor id matches, preferring an
    /// interior cell when both an interior and an exterior one share a name.
    #[must_use]
    pub fn resolve_cell(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.cell_index().cell_by_editor_id(editor_id)
    }

    /// Render lighting for a cell, with CELL XCLL values inherited from LTMP.
    #[cfg(feature = "godot")]
    #[must_use]
    pub(crate) fn cell_lighting(&self, editor_id: &str) -> Option<godot::prelude::VarDictionary> {
        let id = self.resolve_cell(editor_id)?;
        crate::cell_lighting::dictionary(&self.order, id)
    }

    /// The worldspace and editor id whose terrain and object LOD assets back
    /// the requested worldspace, following PNAM's Use LOD Data bit.
    /// xEdit's wbDefinitionsTES5.pas defines this separately from Use Land Data.
    #[must_use]
    pub fn lod_worldspace(&self, world: GlobalFormId) -> Option<(GlobalFormId, String)> {
        let mut lod_world = world;
        let mut visited = std::collections::HashSet::new();
        loop {
            if !visited.insert(lod_world) {
                return None;
            }
            let owner = self.order.winner_ref(lod_world)?;
            let record = self.order.record(owner);
            let localized = self.order.file(owner.file).plugin.header.is_localized();
            let view = soe_esm::records::wrld::World::from_record(record, localized).ok()?;
            if view.parent_flags.is_none_or(|flags| flags & 0x0002 == 0) {
                break;
            }
            lod_world = self.order.resolve(owner.file, view.parent?);
        }
        let editor_id = self
            .order
            .winner(lod_world)
            .and_then(|record| record.editor_id())?;
        Some((lod_world, editor_id))
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
        if self.threads == 1 {
            self.workers.clear();
        }
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
        let _system = soe_trace::SystemScope::new(soe_trace::System::Streaming);
        let _load = soe_trace::LoadScope::operation(
            "cell",
            cell_id.to_string(),
            std::time::Duration::from_millis(33),
        );
        let _span = soe_trace::Span::new("host.stream_cell", "host");
        let mut stats = CellStats::default();
        let _capture = stats.profile.enter();
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
                if models::is_editor_reference(rec) {
                    continue;
                }
                let Ok(refr) = Refr::from_record(rec) else {
                    continue;
                };
                let _reference = soe_trace::Span::new("host.reference", "host");
                stats.references += 1;

                if !soe_game::World::authored_reference_enabled(&self.order, id) {
                    continue;
                }

                let base_id = self.order.resolve(rref.file, refr.base);
                let Some(base_rec) = self.order.winner(base_id) else {
                    continue;
                };
                let Some(model) = models::render_model_for(base_rec) else {
                    continue;
                };
                if base_rec.tag == tag::STAT {
                    stats.statics_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"FURN") {
                    stats.furniture_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"DOOR") {
                    stats.doors_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"ACTI") {
                    stats.activators_with_model += 1;
                } else if base_rec.tag == soe_esm::Tag::new(b"CONT") {
                    stats.containers_with_model += 1;
                }
                work.push((model, refr, id));
            }
            work
        };

        // Counted before building anything, so `build_all` can decide per
        // model whether it clears `batch::INSTANCE_MIN_PLACEMENTS` and is
        // worth keeping in local space for a `MultiMesh`, rather than baking
        // the reference placement in like every other static.
        let mut model_counts: HashMap<String, usize> = HashMap::new();
        for (model, _, _) in &work {
            *model_counts.entry(model.to_lowercase()).or_insert(0) += 1;
        }

        let before = self.textures.counters();
        let results = self.build_all(&work, &model_counts)?;
        let after = self.textures.counters();
        let mut surfaces = Vec::new();
        let mut texture_sets = HashSet::new();
        for ((model, refr, reference), result) in work.iter().zip(results) {
            match result {
                Ok(LoadedStatic::Surfaces {
                    built,
                    marker_shapes_filtered,
                    marker_only,
                    instance_transform,
                }) => {
                    stats.editor_markers_filtered += marker_shapes_filtered;
                    stats.editor_marker_only_models += usize::from(marker_only);
                    stats.meshes_parsed += 1;
                    if built.is_empty() {
                        stats.reference_skips.insert(
                            *reference,
                            if marker_only {
                                "marker model"
                            } else {
                                "no visible geometry"
                            },
                        );
                    }
                    let placement = placement_iso(refr);
                    let identity_base = instance_transform.is_some().then(|| model.to_lowercase());
                    for (shape_index, (arrays, textures)) in built.into_iter().enumerate() {
                        if textures
                            .diffuse
                            .as_ref()
                            .is_none_or(|texture| Arc::ptr_eq(texture, &checker_albedo()))
                        {
                            if let Some(owner) = self.order.winner_ref(*reference) {
                                let base = self.order.resolve(owner.file, refr.base);
                                if let Some(record) = self.order.winner(base) {
                                    soe_trace::warn!(target: "host::materials", system = "host::materials",
                                        reference = %reference, base = %base,
                                        editor_id = ?record.editor_id(), record_type = %record.tag,
                                        model, shape_index, texture_set = ?arrays.material,
                                        "surface has no source albedo");
                                }
                            }
                        }
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
                        let mesh_identity = identity_base
                            .as_ref()
                            .map(|base| Arc::<str>::from(format!("{base}#{shape_index}")));
                        surfaces.push(LoadedSurface {
                            reference: *reference,
                            arrays,
                            textures,
                            placement,
                            instance_transform,
                            mesh_identity,
                        });
                    }
                }
                Err(e) => {
                    soe_trace::warn!(target: "host::data_source", system = "host::data_source", "stream_cell: {model}: {e}");
                    stats.meshes_failed += 1;
                    stats.reference_skips.insert(*reference, "mesh failure");
                }
            }
        }
        stats.distinct_texture_sets = texture_sets.len();
        stats.textures_decoded = after.decoded - before.decoded;
        stats.textures_missing = self.textures.missing_since(before.missing);
        stats.texture_decode_failures = after.failures - before.failures;
        stats.texture_decode_time =
            std::time::Duration::from_nanos(after.decode_ns - before.decode_ns);
        if let Some(water) = self.water_surface(cell_id, &surfaces) {
            stats.water_plane_count += 1;
            stats.triangles += water.arrays.triangle_count();
            surfaces.push(water);
        }
        let mut loaded = LoadedCell { stats, surfaces };
        loaded.refresh_texture_inventory();
        loaded.stats.reference_counts = self.reference_counts(cell_id, &loaded, None);
        for (kind, counts) in &loaded.stats.reference_counts {
            soe_trace::debug!(target: "host::references", cell = %cell_id, base_type = %kind,
                placed = counts.placed, built = counts.built, initially_disabled = counts.initially_disabled,
                enable_parent = counts.enable_parent, runtime_disabled = counts.runtime_disabled,
                markers = counts.markers, other = ?counts.other, "cell reference counts");
        }
        Ok(loaded)
    }

    /// Builds one newly enabled reference without reloading its resident cell.
    ///
    /// # Errors
    /// Propagates missing records and model decoding failures.
    pub fn stream_reference(&self, reference: GlobalFormId) -> Result<LoadedCell, DataSourceError> {
        let winner = self
            .order
            .winner_ref(reference)
            .ok_or_else(|| DataSourceError::CellNotFound(reference.to_string()))?;
        let record = self.order.record(winner);
        if models::is_editor_reference(record) {
            return Ok(LoadedCell {
                stats: CellStats::default(),
                surfaces: Vec::new(),
            });
        }
        let refr = Refr::from_record(record)?;
        let base = self
            .order
            .winner(self.order.resolve(winner.file, refr.base))
            .ok_or_else(|| DataSourceError::CellNotFound(reference.to_string()))?;
        let Some(model) = models::render_model_for(base) else {
            return Ok(LoadedCell {
                stats: CellStats::default(),
                surfaces: Vec::new(),
            });
        };
        self.stream_destruction_model(reference, &model)
    }

    /// Builds the selected destruction-stage model for one placed reference
    /// through the same VFS and NIF path as its base model.
    pub fn stream_destruction_model(
        &self,
        reference: GlobalFormId,
        model: &str,
    ) -> Result<LoadedCell, DataSourceError> {
        let winner = self
            .order
            .winner_ref(reference)
            .ok_or_else(|| DataSourceError::CellNotFound(reference.to_string()))?;
        let refr = Refr::from_record(self.order.record(winner))?;
        let loaded = self.load_static_surfaces(model, &refr, false)?;
        let LoadedStatic::Surfaces {
            built,
            marker_shapes_filtered,
            marker_only,
            ..
        } = loaded;
        let mut stats = CellStats {
            references: 1,
            meshes_parsed: 1,
            editor_markers_filtered: marker_shapes_filtered,
            editor_marker_only_models: usize::from(marker_only),
            ..CellStats::default()
        };
        let surfaces = built
            .into_iter()
            .map(|(arrays, textures)| {
                stats.triangles += arrays.triangle_count();
                LoadedSurface {
                    reference,
                    arrays,
                    textures,
                    placement: placement_iso(&refr),
                    instance_transform: None,
                    mesh_identity: None,
                }
            })
            .collect();
        Ok(LoadedCell { stats, surfaces })
    }

    /// The cell's water plane, built from its own `CELL` record rather than
    /// the reference scan above: interior cells with water (a dungeon lake)
    /// carry no `REFR` for it, `XCLW` and `XCWT` sit on the cell itself.
    /// An exterior cell without its own `XCLW`, or with xEdit's "Default"
    /// marker there, takes its worldspace's `DNAM` height and `NAM2` water,
    /// which is how the open sea gets its plane.
    /// `None` for a cell with no water flag, no height from either, or whose
    /// own record cannot be resolved, in which case the cell still streams
    /// normally with no water surface rather than failing the whole call.
    fn water_surface(
        &self,
        cell_id: GlobalFormId,
        surfaces: &[LoadedSurface],
    ) -> Option<LoadedSurface> {
        let rref = self.order.winner_ref(cell_id)?;
        let rec = self.order.record(rref);
        if rec.tag != tag::CELL {
            return None;
        }
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        let view = Cell::from_record(rec, localized).ok()?;
        if !view.has_water() {
            return None;
        }
        let cell_height = view.water_height.filter(|h| water::is_recorded_height(*h));
        let world_water = match (view.grid, view.world) {
            (Some(_), Some(raw)) => {
                water::world_water(&self.order, self.order.resolve(rref.file, raw))
            }
            _ => water::WorldWater::default(),
        };
        let height = cell_height.or(world_water.height)?;
        let water_id = view
            .water
            .map(|raw| self.order.resolve(rref.file, raw))
            .or(world_water.water);
        let material = water_id
            .and_then(|id| self.order.winner(id))
            .map(water_material_from);
        let footprint = match view.grid {
            Some(grid) => {
                let min_x = grid.x as f32 * crate::units::CELL_UNITS;
                let min_y = grid.y as f32 * crate::units::CELL_UNITS;
                (
                    min_x,
                    min_y,
                    min_x + crate::units::CELL_UNITS,
                    min_y + crate::units::CELL_UNITS,
                )
            }
            None => interior_water_footprint(surfaces),
        };
        let data = water_plane_mesh(footprint, height, material);
        let arrays = build_surface(&data, MeshOptions::default()).ok()?;
        Some(LoadedSurface {
            reference: water_id.unwrap_or(cell_id),
            arrays,
            textures: SurfaceTextures::default(),
            placement: soe_physics::Iso::IDENTITY,
            instance_transform: None,
            mesh_identity: None,
        })
    }

    /// Parse and build every model, in order, on `threads` workers over
    /// chunks of the work list, textures included. The archive view and the
    /// texture cache are shared read-only and under a mutex respectively.
    fn build_all(
        &self,
        work: &[(String, Refr, GlobalFormId)],
        model_counts: &HashMap<String, usize>,
    ) -> Result<Vec<BuiltStatic>, DataSourceError> {
        let _span = soe_trace::Span::with_payload("host.build_all", "host", work.len() as u64);
        self.workers
            .ordered_map(work, self.threads, |(model, refr, reference)| {
                let instancing = model_counts
                    .get(&model.to_lowercase())
                    .copied()
                    .unwrap_or(0)
                    >= crate::batch::INSTANCE_MIN_PLACEMENTS;
                let base = self
                    .order
                    .winner_ref(*reference)
                    .map(|rref| self.order.resolve(rref.file, refr.base));
                let _object = soe_trace::LoadScope::object(format!(
                    "base:{} mesh:{model}",
                    base.map_or_else(|| "unknown".to_owned(), |base| base.to_string())
                ));
                self.load_static_surfaces(model, refr, instancing)
            })
    }

    /// Reads and builds every shape of `model` at `refr`'s placement.
    /// `instancing` comes from a placement count `build_all` already took
    /// over the whole cell: when true, the shapes keep their own local
    /// transform only, dropping the reference placement, and the caller
    /// gets that placement back out through `LoadedStatic::Surfaces`'s
    /// `instance_transform` to apply per instance through a `MultiMesh`
    /// instead. Every other static keeps baking the full placement into its
    /// vertex data, unchanged.
    fn load_static_surfaces(&self, model: &str, refr: &Refr, instancing: bool) -> BuiltStatic {
        let _span = soe_trace::Span::new("host.static", "host");
        let path = format!("meshes/{model}");
        let bytes = self.vfs.read(&path)?;
        let file = {
            let _parse = soe_trace::LoadMeasure::new(
                soe_trace::LoadStage::NifParse,
                &soe_bsa::normalize(&path),
            );
            let _scope =
                soe_trace::LoadScope::new("mesh_parse", soe_trace::LoadCause::Parse, path.clone());
            placed_model(&bytes)?
        };
        let _build = soe_trace::LoadScope::new("mesh_build", soe_trace::LoadCause::CpuBuild, path);
        let marker_blocks = editor_marker_shape_blocks(&file);
        let nonvisual_blocks = nonvisual_shape_indices(&file.blocks);
        let mut meshes = file.meshes()?;
        let marker_shapes_filtered = meshes
            .iter()
            .filter(|mesh| marker_blocks.contains(&mesh.block))
            .count();
        meshes.retain(|mesh| {
            !marker_blocks.contains(&mesh.block) && !nonvisual_blocks.contains(&mesh.block)
        });
        let marker_only = marker_shapes_filtered > 0 && meshes.is_empty();
        let fire_palette = infer_fire_palette(model, &meshes);
        let refr_transform = placement_transform(refr);
        let mut out = Vec::with_capacity(meshes.len());
        for shape in &meshes {
            let arrays = {
                let _surface = soe_trace::Span::new("host.build_surface", "host");
                let combined = if instancing {
                    shape.transform
                } else {
                    refr_transform.then(&shape.transform)
                };
                let data = mesh_data_from(shape, &combined);
                let mut arrays = build_surface(&data, MeshOptions::default())?;
                apply_fire_palette_fallback(&mut arrays.material, fire_palette.as_deref());
                arrays
            };
            let textures = {
                let _material = soe_trace::Span::new("host.material", "host");
                self.textures.resolve(&self.vfs, &arrays.material)
            };
            out.push((arrays, textures));
        }
        let instance_transform = instancing.then(|| {
            crate::units::placement_to_godot(
                refr_transform.rotation.rows,
                refr_transform.translation.to_array(),
                refr_transform.scale,
            )
        });
        Ok(LoadedStatic::Surfaces {
            built: out,
            marker_shapes_filtered,
            marker_only,
            instance_transform,
        })
    }
}

fn infer_fire_palette(model: &str, meshes: &[NifMesh]) -> Option<String> {
    if !model.to_ascii_lowercase().contains("fire") {
        return None;
    }
    meshes.iter().find_map(|mesh| {
        let effect = mesh.material.effect.as_ref()?;
        (mesh.material.flags1 & soe_nif::blocks::flags1::GREYSCALE_TO_PALETTE_COLOR != 0
            && !effect.greyscale_texture.is_empty())
        .then(|| effect.greyscale_texture.clone())
    })
}

fn apply_fire_palette_fallback(material: &mut MaterialDesc, palette: Option<&str>) {
    use soe_nif::blocks::{flags1, BlendFactor};
    let Some(palette) = palette else {
        return;
    };
    if material.shader_flags_1 & flags1::GREYSCALE_TO_PALETTE_COLOR != 0 {
        return;
    }
    let Some(effect) = material.effect.as_mut() else {
        return;
    };
    if !effect.params.greyscale_texture.is_empty()
        || !effect.blend.is_some_and(|blend| {
            matches!(
                (blend.src, blend.dst),
                (BlendFactor::SrcAlpha | BlendFactor::One, BlendFactor::One)
            )
        })
    {
        return;
    }
    material.shader_flags_1 |= flags1::GREYSCALE_TO_PALETTE_COLOR;
    effect.params.greyscale_texture = palette.to_string();
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
        /// The reference's placement, in Godot space, when `load_static_surfaces`
        /// left `built`'s vertex data in local space for a `MultiMesh`. `None`
        /// means the usual path: the placement is already baked into `built`.
        instance_transform: Option<crate::units::GodotTransform>,
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

/// OpenMW's NiAVObject::Flag_Hidden excludes bounds and trigger meshes.
/// Shaderless Skyrim shapes are emitter/helper geometry. NIF water shaders
/// have no host material binding, so they must not become opaque white meshes.
fn nonvisual_shape_indices(blocks: &[soe_nif::blocks::Block]) -> HashSet<usize> {
    use soe_nif::blocks::Block;
    let unsupported = |shader: soe_nif::BlockRef| {
        shader.get().is_none()
            || matches!(
                shader.get().and_then(|index| blocks.get(index)),
                Some(Block::Unknown { type_name, .. }) if type_name == "BSWaterShaderProperty"
            )
    };
    let mut hidden = HashSet::new();
    for (index, block) in blocks.iter().enumerate() {
        let (av, unsupported_shader) = match block {
            Block::Node(node) => (&node.av, false),
            Block::TriShape(shape) => (&shape.av, unsupported(shape.shader)),
            Block::BsTriShape(shape) => (&shape.av, unsupported(shape.shader)),
            _ => continue,
        };
        if av.flags & 1 != 0 || unsupported_shader {
            mark_descendant_shapes(blocks, index, &mut hidden);
        }
    }
    hidden
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
    // CONTEXT: nif.xml and nifly append Min, Max, Current to BSDamageStage.
    // Select the intact range before ordinary hidden-subtree filtering.
    if file
        .header
        .block_types
        .iter()
        .any(|kind| kind == "BSDamageStage")
    {
        let mut stream = soe_nif::stream::Stream::new(bytes);
        soe_nif::header::read_header(&mut stream)?;
        for (index, size) in file.header.block_sizes.iter().enumerate() {
            let data = stream.take(*size as usize)?;
            if file.header.block_type(index) == "BSDamageStage" {
                if let Some(Block::Node(node)) = file.blocks.get_mut(index) {
                    if data.get(data.len().saturating_sub(3)) == Some(&0) {
                        node.av.flags &= !1;
                    } else {
                        node.av.flags |= 1;
                    }
                }
            }
        }
    }
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
    let rotation = Mat33 {
        rows: crate::units::skyrim_euler_matrix(t.rotation),
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
        colors: shape.material.effect.as_ref().map(|_| {
            if shape.colors.is_empty() {
                vec![[1.0; 4]; shape.positions.len()]
            } else {
                shape.colors.clone()
            }
        }),
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
        water: None,
        effect: m.effect.as_ref().map(|params| crate::mesh::EffectMaterial {
            params: params.clone(),
            blend: m.blend,
            alpha_test: m.alpha_test,
            uv_offset: m.uv_offset,
            uv_scale: m.uv_scale,
        }),
    }
}

/// An interior cell's water has no `XCLC` grid to size a plane from, so the
/// footprint comes from the cell's own geometry instead: the union, in
/// Skyrim units, of the bounds every surface already built for this cell
/// reports in Godot space. A cell whose only content is its water (nothing
/// streamed before this call, or every reference failed to build) falls
/// back to one exterior-cell-sized square centred on the origin, wide
/// enough to be visible and cheap enough that it never distorts a stats
/// count meaningfully.
fn interior_water_footprint(surfaces: &[LoadedSurface]) -> (f32, f32, f32, f32) {
    let bounds = surfaces
        .iter()
        .map(|s| s.arrays.bounds)
        .reduce(crate::batch::merge_bounds);
    match bounds {
        Some(b) => {
            let min = b.min.to_skyrim();
            let max = b.max.to_skyrim();
            let (min_x, max_x) = (min.x.min(max.x), min.x.max(max.x));
            let (min_y, max_y) = (min.y.min(max.y), min.y.max(max.y));
            (min_x, min_y, max_x, max_y)
        }
        None => {
            let half = crate::units::CELL_UNITS / 2.0;
            (-half, -half, half, half)
        }
    }
}

/// A single flat quad at `height`, covering `(min_x, min_y, max_x, max_y)`
/// in Skyrim units, with a `MaterialDesc` carrying `material` for the
/// Godot layer to draw with once it maps `WaterMaterial` onto a shader.
/// Winding matches `terrain_surface_from_heights`'s row-major quad so
/// `MeshOptions::default`'s `flip_winding` produces the same up-facing
/// result as terrain and every other NIF-sourced surface.
fn water_plane_mesh(
    (min_x, min_y, max_x, max_y): (f32, f32, f32, f32),
    height: f32,
    material: Option<WaterMaterial>,
) -> MeshData {
    let positions = vec![
        [min_x, min_y, height],
        [max_x, min_y, height],
        [min_x, max_y, height],
        [max_x, max_y, height],
    ];
    let uvs = Some(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]]);
    MeshData {
        positions,
        normals: None,
        tangents: None,
        uvs,
        indices: vec![0, 1, 3, 0, 3, 2],
        colors: None,
        bone_indices: None,
        bone_weights: None,
        material: MaterialDesc {
            water: material,
            ..MaterialDesc::default()
        },
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
mod grass_tests;

#[cfg(test)]
mod marker_shape_tests;

#[cfg(test)]
mod animated_static_mesh_tests;

#[cfg(test)]
mod rnd_17_tests;

#[cfg(test)]
mod rnd_22_tests;
