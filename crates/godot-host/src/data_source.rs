// SPDX-License-Identifier: GPL-3.0-or-later

//! A real Skyrim SE `Data` folder as a source of cells and meshes: the
//! `esm`, `bsa` and `nif` crates wired together and fed into `mesh::build_surface`.
//!
//! `DataSource::open` loads the five base masters through a `LoadOrder`,
//! builds the `CellIndex` over them, and builds a `bsa::Vfs` from their
//! associated archives plus the data folder as a loose overlay.
//! `DataSource::stream_cell` resolves a cell by editor id through the index,
//! walks the `REFR` ids the index files under it, and for each one whose base is a `STAT`
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use soe_bsa::{find_associated_archives, Vfs};
use soe_dds::{Dds, Image as DdsImage};
use soe_esm::records::cell::Cell;
use soe_esm::records::land::Land;
use soe_esm::records::refr::Refr;
use soe_esm::records::stat::Static;
use soe_esm::records::wrld::World;
use soe_esm::{tag, CellIndex, GlobalFormId, LoadOrder, Record};
use soe_nif::{Mat33, Mesh as NifMesh, Transform as NifTransform, Vec3 as NifVec3};

use crate::mesh::{
    build_surface, AlphaMode, MaterialDesc, MeshData, MeshError, MeshOptions, SurfaceArrays,
};

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
    #[error("cell '{0}' was not found in the load order")]
    CellNotFound(String),
}

/// Counts from one `stream_cell` run, for the headless smoke test and the
/// gated real-data test to report. Not `Copy`, `textures_missing` is a list.
#[derive(Debug, Clone, Default)]
pub struct CellStats {
    pub references: usize,
    pub statics_with_model: usize,
    pub meshes_parsed: usize,
    pub meshes_failed: usize,
    pub triangles: usize,
    /// Distinct (diffuse, normal) texture path pairs seen across the cell's
    /// materials, lowercased. This is the number of `StandardMaterial3D`
    /// instances the Godot layer builds, one per set.
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

/// A surface's diffuse and normal textures, ready to upload. `None` means
/// the material named no texture in that slot, or the read or decode
/// failed, counted in `CellStats` either way. `Arc` because a texture shared
/// by many statics is read once on whichever worker thread reaches it first
/// and cloned out of `TextureCache` cheaply for every surface that uses it.
#[derive(Debug, Clone, Default)]
pub struct SurfaceTextures {
    pub diffuse: Option<Arc<TextureData>>,
    /// Skyrim packs specular into this texture's alpha channel. The RGB
    /// channels alone are the normal map, alpha is not a transparency mask
    /// here even though the decoded form carries one.
    pub normal: Option<Arc<TextureData>>,
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

/// A cell's `LAND` heights as one surface in Skyrim units, 32 by 32 quads
/// of two triangles, wound counter-clockwise seen from above so
/// `build_surface`'s winding flip lands them the same way as a NIF's faces.
/// `VNML` normals are used when present, generated otherwise. `None` when
/// the record has no height block.
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

/// A block compression Godot uploads as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFormat {
    Bc1,
    Bc2,
    Bc3,
    Bc4,
    Bc5,
    Bc7,
}

/// A texture as the Godot layer uploads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextureData {
    /// The DDS payload of layer 0 as stored, every mip level back to back
    /// and padded to a 1 by 1 tail, so the upload is a copy and the GPU
    /// samples the file's own mip chain. `mip0_len` lets the layer fall
    /// back to the base level alone if Godot still rejects the chain.
    Compressed {
        width: u32,
        height: u32,
        format: BlockFormat,
        mips: u32,
        mip0_len: usize,
        bytes: Vec<u8>,
    },
    /// The top mip decoded to RGBA8, for formats Godot has no block type
    /// for (uncompressed DDS, signed BC4 and BC5, volumes).
    Rgba8(DdsImage),
}

impl TextureData {
    pub fn width(&self) -> u32 {
        match self {
            TextureData::Compressed { width, .. } => *width,
            TextureData::Rgba8(image) => image.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            TextureData::Compressed { height, .. } => *height,
            TextureData::Rgba8(image) => image.height,
        }
    }
}

/// Reads a texture through `vfs`. A block-compressed file is passed through
/// with its mip chain and never decoded, which is what makes the material
/// path cheap: the trace of 2026-09-18 had BCn decode at 97 percent of a
/// cell's stream time. Anything else decodes its top mip to RGBA8.
fn load_texture(vfs: &Vfs, path: &str) -> Result<TextureData, DataSourceError> {
    let bytes = vfs.read(path)?;
    let dds = Dds::parse(&bytes)?;
    let format = match dds.format() {
        soe_dds::Format::Bc1 => Some(BlockFormat::Bc1),
        soe_dds::Format::Bc2 => Some(BlockFormat::Bc2),
        soe_dds::Format::Bc3 => Some(BlockFormat::Bc3),
        soe_dds::Format::Bc4 { signed: false } => Some(BlockFormat::Bc4),
        soe_dds::Format::Bc5 { signed: false } => Some(BlockFormat::Bc5),
        soe_dds::Format::Bc7 => Some(BlockFormat::Bc7),
        _ => None,
    };
    let flat = dds.level(0, 0).map(|l| l.depth == 1).unwrap_or(false);
    let Some(format) = format.filter(|_| flat) else {
        return Ok(TextureData::Rgba8(dds.decode(0, 0)?));
    };
    let stored = dds.mip_count().max(1);
    let mut payload = Vec::new();
    for mip in 0..stored {
        payload.extend_from_slice(dds.level_bytes(0, mip)?);
    }
    // Godot wants the chain to reach 1 by 1 and refuses a shorter one. The
    // vanilla files stop at 2 by 2 or 4 by 4, so the missing levels, each
    // a block or two of at most four pixels a side, repeat the last block
    // the file holds.
    let (width, height) = (dds.width(), dds.height());
    let full = 32 - width.max(height).max(1).leading_zeros();
    let block = match format {
        BlockFormat::Bc1 | BlockFormat::Bc4 => 8,
        _ => 16,
    };
    let last_block: Vec<u8> = payload[payload.len().saturating_sub(block)..].to_vec();
    for mip in stored..full {
        let blocks_wide = (width >> mip).max(1).div_ceil(4) as usize;
        let blocks_high = (height >> mip).max(1).div_ceil(4) as usize;
        for _ in 0..blocks_wide * blocks_high {
            payload.extend_from_slice(&last_block);
        }
    }
    Ok(TextureData::Compressed {
        width,
        height,
        format,
        mips: full.max(stored),
        mip0_len: dds.level(0, 0)?.len,
        bytes: payload,
    })
}

/// Decoded textures keyed by lowercase path, owned by the `DataSource` and
/// shared by every stream call and every worker thread, so a texture used by
/// forty statics across nine cells decodes once for the source's lifetime.
/// The counters run since open. A stream call reads them before and after
/// to fill its own `CellStats`, which is exact as long as stream calls on
/// one source do not overlap, and they do not, a stream call holds `&self`
/// for its whole duration on one thread.
#[derive(Default)]
struct TextureCache {
    /// One slot per lowercase path. The map lock is held only to find or
    /// create the slot, the read itself runs under the slot's `OnceLock`, so
    /// two workers reaching one texture at once share a single read instead
    /// of both doing it (the first version raced and read 132 files for 88
    /// textures).
    textures: Mutex<HashMap<String, TextureSlot>>,
    decoded: AtomicUsize,
    failures: AtomicUsize,
    decode_ns: AtomicU64,
    missing: Mutex<Vec<String>>,
}

/// A texture read at most once, `None` after a miss or a failed read.
type TextureSlot = Arc<OnceLock<Option<Arc<TextureData>>>>;

#[derive(Clone, Copy)]
struct TextureCounters {
    decoded: usize,
    failures: usize,
    decode_ns: u64,
    missing: usize,
}

impl TextureCache {
    fn resolve(&self, vfs: &Vfs, material: &MaterialDesc) -> SurfaceTextures {
        SurfaceTextures {
            diffuse: material
                .diffuse
                .as_deref()
                .and_then(|p| self.get_or_decode(vfs, p)),
            normal: material
                .normal
                .as_deref()
                .and_then(|p| self.get_or_decode(vfs, p)),
        }
    }

    fn get_or_decode(&self, vfs: &Vfs, path: &str) -> Option<Arc<TextureData>> {
        let slot = self
            .textures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(path.to_lowercase())
            .or_default()
            .clone();
        slot.get_or_init(|| self.read(vfs, path)).clone()
    }

    fn read(&self, vfs: &Vfs, path: &str) -> Option<Arc<TextureData>> {
        let _span = soe_trace::Span::new("host.texture_decode", "host");
        let started = std::time::Instant::now();
        let result = load_texture(vfs, path);
        self.decode_ns
            .fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        match result {
            Ok(image) => {
                self.decoded.fetch_add(1, Ordering::Relaxed);
                Some(Arc::new(image))
            }
            Err(DataSourceError::Bsa(soe_bsa::Error::NotFound { .. })) => {
                self.missing
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(path.to_string());
                None
            }
            Err(e) => {
                eprintln!("stream_cell: texture {path}: {e}");
                self.failures.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    fn counters(&self) -> TextureCounters {
        TextureCounters {
            decoded: self.decoded.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            decode_ns: self.decode_ns.load(Ordering::Relaxed),
            missing: self
                .missing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len(),
        }
    }

    fn missing_since(&self, count: usize) -> Vec<String> {
        self.missing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(count..)
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    }
}

/// An opened `Data` folder: the load order and the archive and loose file
/// view built from it.
pub struct DataSource {
    order: LoadOrder,
    index: CellIndex,
    vfs: Vfs,
    threads: usize,
    textures: TextureCache,
}

/// Parse the base masters, on `threads` workers when more than one, and hand
/// them back in master order so the load order stays deterministic.
fn parse_masters(data_dir: &Path, threads: usize) -> Vec<Result<soe_esm::Plugin, DataSourceError>> {
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
    if threads <= 1 {
        return BASE_MASTERS.iter().map(|name| parse_one(name)).collect();
    }
    let mut out = Vec::with_capacity(BASE_MASTERS.len());
    std::thread::scope(|scope| {
        let handles: Vec<_> = BASE_MASTERS
            .iter()
            .map(|name| scope.spawn(move || parse_one(name)))
            .collect();
        for handle in handles {
            out.push(handle.join().unwrap_or_else(|_| {
                Err(DataSourceError::CellNotFound(
                    "a plugin parse thread panicked".into(),
                ))
            }));
        }
    });
    out
}

impl DataSource {
    /// Loads the base masters and their associated archives from `data_dir`,
    /// then adds `data_dir` itself as a loose overlay so a texture or mesh
    /// replacer placed there beats the archives, matching the shipped engine.
    /// Parsing and mesh building use the machine's parallelism.
    pub fn open(data_dir: &Path) -> Result<Self, DataSourceError> {
        let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
        Self::open_with_threads(data_dir, threads)
    }

    /// `open` with a fixed worker count, one for the sequential path.
    pub fn open_with_threads(data_dir: &Path, threads: usize) -> Result<Self, DataSourceError> {
        let _span = soe_trace::Span::with_payload("host.open", "host", threads as u64);
        let threads = threads.max(1);
        let parsed = parse_masters(data_dir, threads);
        let mut order = LoadOrder::new();
        for (name, plugin) in BASE_MASTERS.iter().zip(parsed) {
            order
                .add(name, plugin?)
                .map_err(|source| DataSourceError::Plugin {
                    name: (*name).to_string(),
                    source,
                })?;
        }
        let index = CellIndex::build(&order);
        let mut vfs = Vfs::new();
        {
            let _archives = soe_trace::Span::new("host.index_archives", "host");
            for name in BASE_MASTERS {
                for archive_path in find_associated_archives(name, data_dir)? {
                    vfs.add_archive_path(&archive_path)?;
                }
            }
            vfs.add_loose_root(data_dir)?;
        }
        Ok(DataSource {
            order,
            index,
            vfs,
            threads,
            textures: TextureCache::default(),
        })
    }

    /// The cell index built at open, for callers that walk cells by grid
    /// position or need a cell's land and navmeshes.
    #[must_use]
    pub fn cell_index(&self) -> &CellIndex {
        &self.index
    }

    /// The winning `CELL` record whose editor id matches, preferring an
    /// interior cell when both an interior and an exterior one share a name.
    #[must_use]
    pub fn resolve_cell(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.index.cell_by_editor_id(editor_id)
    }

    /// The first interior cell, by form id, whose reference count exceeds
    /// `min_refs`. Used when a named cell such as `WhiterunBanneredMare` is
    /// missing from an install, so the real-data test still has a populated
    /// interior to load.
    #[must_use]
    pub fn first_populated_interior_cell(&self, min_refs: usize) -> Option<GlobalFormId> {
        let mut candidates: Vec<GlobalFormId> = self
            .index
            .cells()
            .filter(|&(id, n)| n > min_refs && self.index.is_interior(id))
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

    /// The square of exterior cells within `radius` grid steps of the cell
    /// named `editor_id`, in that cell's worldspace, sorted south to north
    /// then west to east. Radius 2 is the game's `uGridsToLoad=5`. One pass
    /// over the winning `CELL` records, which is cheap next to the reference
    /// scan since there are about fifty thousand cells against a million
    /// references. Fails when the editor id names no exterior cell.
    pub fn resolve_exterior_block(
        &self,
        editor_id: &str,
        radius: i32,
    ) -> Result<ExteriorBlock, DataSourceError> {
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
        let in_range = |x: i32, y: i32| (x - ox).abs() <= radius && (y - oy).abs() <= radius;
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
    fn land_parent_of(&self, world: GlobalFormId) -> Option<GlobalFormId> {
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
        let mut cells = Vec::with_capacity(block.cells.len());
        let mut land_cells = 0;
        for cell in &block.cells {
            let mut loaded = self.stream_cell_id(cell.id)?;
            let own_or_parent = land
                .get(&cell.id)
                .or_else(|| parent_at(cell.x, cell.y).and_then(|id| land.get(&id)));
            if let Some((land_id, land)) = own_or_parent {
                let _terrain = soe_trace::Span::new("host.terrain_surface", "host");
                match terrain_surface(cell, land) {
                    Ok(Some(arrays)) => {
                        land_cells += 1;
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
            cells.push((*cell, loaded));
        }
        Ok(LoadedExterior {
            block,
            cells,
            land_cells,
        })
    }

    /// Walks every `REFR` the index files under `cell_id`, builds a
    /// `SurfaceArrays` for each `STAT` reference with a model, and reports
    /// what it found. `CellStats::references` counts `REFR` only, actors and
    /// projectiles the index also holds are skipped here.
    /// A reference whose mesh fails to read or parse is counted as failed and
    /// skipped, it does not stop the rest of the cell from loading.
    /// Worker threads for NIF parsing and mesh building. One means the
    /// sequential path. Defaults to the machine's parallelism.
    pub fn set_threads(&mut self, threads: usize) {
        self.threads = threads.max(1);
    }

    #[must_use]
    pub fn threads(&self) -> usize {
        self.threads
    }

    pub fn stream_cell_id(&self, cell_id: GlobalFormId) -> Result<LoadedCell, DataSourceError> {
        let _span = soe_trace::Span::new("host.stream_cell", "host");
        let mut stats = CellStats::default();
        let work = {
            let _scan = soe_trace::Span::with_payload(
                "host.scan_references",
                "host",
                self.index.references(cell_id).len() as u64,
            );
            let mut work: Vec<(String, Refr, GlobalFormId)> = Vec::new();
            for id in self.index.references(cell_id).iter().copied() {
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
                if base_rec.tag != tag::STAT {
                    continue;
                }
                let Ok(stat) = Static::from_record(base_rec) else {
                    continue;
                };
                let Some(model) = stat.model.filter(|m| !m.is_empty()) else {
                    continue;
                };
                stats.statics_with_model += 1;
                work.push((model, refr, id));
            }
            work
        };

        let before = self.textures.counters();
        let results = self.build_all(&work);
        let after = self.textures.counters();
        let mut surfaces = Vec::new();
        let mut texture_sets: HashSet<(Option<String>, Option<String>)> = HashSet::new();
        for ((model, _, reference), result) in work.iter().zip(results) {
            match result {
                Ok(built) => {
                    stats.meshes_parsed += 1;
                    for (arrays, textures) in built {
                        stats.triangles += arrays.triangle_count();
                        texture_sets.insert((
                            arrays.material.diffuse.as_deref().map(str::to_lowercase),
                            arrays.material.normal.as_deref().map(str::to_lowercase),
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
    fn build_all(&self, work: &[(String, Refr, GlobalFormId)]) -> Vec<BuiltStatic> {
        let _span = soe_trace::Span::with_payload("host.build_all", "host", work.len() as u64);
        let threads = self.threads.min(work.len().max(1));
        if threads <= 1 {
            return work
                .iter()
                .map(|(model, refr, _)| self.load_static_surfaces(model, refr))
                .collect();
        }
        let chunk = work.len().div_ceil(threads);
        let mut results: Vec<BuiltStatic> = Vec::with_capacity(work.len());
        std::thread::scope(|scope| {
            let handles: Vec<_> = work
                .chunks(chunk)
                .map(|items| {
                    scope.spawn(move || {
                        let _worker = soe_trace::Span::with_payload(
                            "host.worker",
                            "host",
                            items.len() as u64,
                        );
                        items
                            .iter()
                            .map(|(model, refr, _)| self.load_static_surfaces(model, refr))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                match handle.join() {
                    Ok(part) => results.extend(part),
                    Err(_) => results.push(Err(DataSourceError::CellNotFound(
                        "a worker thread panicked".into(),
                    ))),
                }
            }
        });
        results
    }

    fn load_static_surfaces(&self, model: &str, refr: &Refr) -> BuiltStatic {
        let _span = soe_trace::Span::new("host.static", "host");
        let path = format!("meshes/{model}");
        let bytes = self.vfs.read(&path)?;
        let file = soe_nif::parse(&bytes)?;
        let meshes = file.meshes()?;
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
        Ok(out)
    }
}

/// One static's surfaces with their textures, or why it failed.
type BuiltStatic = Result<Vec<(SurfaceArrays, SurfaceTextures)>, DataSourceError>;

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

/// Bakes `combined` (the reference's placement composed with the shape's own
/// transform inside its NIF) into the shape's vertex data and adapts the
/// `nif` crate's neutral mesh to godot-host's `MeshSource` shape. Bone
/// indices widen from the file's u8 to the four-bone `u16` layout `mesh::MeshData`
/// carries, since a skeleton can exceed 256 bones.
fn mesh_data_from(shape: &NifMesh, combined: &NifTransform) -> MeshData {
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
        environment: m.environment().map(str::to_string),
        shader_flags_1: m.flags1,
        shader_flags_2: m.flags2,
        alpha,
        double_sided: m.double_sided,
    }
}
