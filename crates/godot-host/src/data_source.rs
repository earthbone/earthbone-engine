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
//! win over archives there, same as meshes). `Dds::parse` and `decode` turn
//! the bytes into RGBA8, top mip only, and `DataSource::texture_cache` keeps
//! one decode per lowercase path alive for the life of the `DataSource`, so a
//! texture shared by many statics in a cell decodes once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use soe_bsa::{find_associated_archives, Vfs};
use soe_dds::{Dds, Image as DdsImage};
use soe_esm::records::refr::Refr;
use soe_esm::records::stat::Static;
use soe_esm::{tag, CellIndex, GlobalFormId, LoadOrder};
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
    /// Wall time spent inside `Vfs::read` and `Dds::decode` for every texture
    /// actually read, cache hits excluded. Archive indexing and load order
    /// setup are outside this, they are counted once in the surrounding
    /// `stream_cell_id` wall time the caller times itself.
    pub texture_decode_time: std::time::Duration,
}

/// One built surface with its material's textures decoded to RGBA8, ready
/// for the Godot layer to upload. Kept apart from `SurfaceArrays`, which the
/// synthetic streaming path also builds and has no `Vfs` to read textures
/// from.
pub struct LoadedSurface {
    pub arrays: SurfaceArrays,
    pub textures: SurfaceTextures,
}

/// A surface's diffuse and normal textures, decoded to RGBA8 at the top mip.
/// `None` means the material named no texture in that slot, or the read or
/// decode failed, counted in `CellStats` either way. `Rc` because a texture
/// shared by many statics in a cell is decoded once and cloned out of
/// `MaterialCache` cheaply for every surface that uses it.
#[derive(Debug, Clone, Default)]
pub struct SurfaceTextures {
    pub diffuse: Option<Rc<DdsImage>>,
    /// Skyrim packs specular into this texture's alpha channel. The RGB
    /// channels alone are the normal map, alpha is not a transparency mask
    /// here even though `DdsImage` carries one.
    pub normal: Option<Rc<DdsImage>>,
}

pub struct LoadedCell {
    pub stats: CellStats,
    pub surfaces: Vec<LoadedSurface>,
}

/// Reads a texture through `vfs` and decodes its top mip (layer 0, mip 0) to
/// RGBA8. Windowed capture and the real-cell test only ever need the base
/// level, a streamed mip chain is future work once distance-based texture
/// budgets exist.
fn load_texture(vfs: &Vfs, path: &str) -> Result<DdsImage, DataSourceError> {
    let bytes = vfs.read(path)?;
    let dds = Dds::parse(&bytes)?;
    Ok(dds.decode(0, 0)?)
}

/// Decodes and caches a cell's materials' textures, keyed by lowercase path
/// so a texture shared by many references decodes once. Lives for one
/// `stream_cell_id` call, not on `DataSource` itself, since it also tallies
/// that run's texture stats.
#[derive(Default)]
struct MaterialCache {
    textures: HashMap<String, Option<Rc<DdsImage>>>,
    texture_sets: std::collections::HashSet<(Option<String>, Option<String>)>,
    textures_missing: Vec<String>,
    texture_decode_failures: usize,
    textures_decoded: usize,
    texture_decode_time: std::time::Duration,
}

impl MaterialCache {
    fn resolve(&mut self, vfs: &Vfs, material: &MaterialDesc) -> SurfaceTextures {
        let set_key = (
            material.diffuse.as_deref().map(str::to_lowercase),
            material.normal.as_deref().map(str::to_lowercase),
        );
        self.texture_sets.insert(set_key);
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

    fn get_or_decode(&mut self, vfs: &Vfs, path: &str) -> Option<Rc<DdsImage>> {
        let key = path.to_lowercase();
        if let Some(cached) = self.textures.get(&key) {
            return cached.clone();
        }
        let started = std::time::Instant::now();
        let result = load_texture(vfs, path);
        self.texture_decode_time += started.elapsed();
        let value = match result {
            Ok(image) => {
                self.textures_decoded += 1;
                Some(Rc::new(image))
            }
            Err(DataSourceError::Bsa(soe_bsa::Error::NotFound { .. })) => {
                self.textures_missing.push(path.to_string());
                None
            }
            Err(e) => {
                eprintln!("stream_cell: texture {path}: {e}");
                self.texture_decode_failures += 1;
                None
            }
        };
        self.textures.insert(key, value.clone());
        value
    }
}

/// An opened `Data` folder: the load order and the archive and loose file
/// view built from it.
pub struct DataSource {
    order: LoadOrder,
    index: CellIndex,
    vfs: Vfs,
    threads: usize,
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
            let mut work: Vec<(String, Refr)> = Vec::new();
            for id in self.index.references(cell_id) {
                let Some(rref) = self.order.winner_ref(*id) else {
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
                work.push((model, refr));
            }
            work
        };

        let results = self.build_all(&work);
        let mut surfaces = Vec::new();
        // Workers only build geometry. Textures resolve here, after the join,
        // because the cache hands out `Rc` images and tallies per-cell stats.
        let mut materials = MaterialCache::default();
        for (model, result) in work.iter().map(|(m, _)| m).zip(results) {
            match result {
                Ok(built) => {
                    stats.meshes_parsed += 1;
                    for arrays in built {
                        let _resolve = soe_trace::Span::new("host.material", "host");
                        stats.triangles += arrays.triangle_count();
                        let textures = materials.resolve(&self.vfs, &arrays.material);
                        surfaces.push(LoadedSurface { arrays, textures });
                    }
                }
                Err(e) => {
                    eprintln!("stream_cell: {model}: {e}");
                    stats.meshes_failed += 1;
                }
            }
        }
        stats.distinct_texture_sets = materials.texture_sets.len();
        stats.textures_decoded = materials.textures_decoded;
        stats.textures_missing = materials.textures_missing;
        stats.texture_decode_failures = materials.texture_decode_failures;
        stats.texture_decode_time = materials.texture_decode_time;
        Ok(LoadedCell { stats, surfaces })
    }

    /// Parse and build every model, in order, on `threads` workers over
    /// chunks of the work list. The archive view is shared read-only.
    fn build_all(
        &self,
        work: &[(String, Refr)],
    ) -> Vec<Result<Vec<SurfaceArrays>, DataSourceError>> {
        let _span = soe_trace::Span::with_payload("host.build_all", "host", work.len() as u64);
        let threads = self.threads.min(work.len().max(1));
        if threads <= 1 {
            return work
                .iter()
                .map(|(model, refr)| self.load_static_surfaces(model, refr))
                .collect();
        }
        let chunk = work.len().div_ceil(threads);
        let mut results: Vec<Result<Vec<SurfaceArrays>, DataSourceError>> =
            Vec::with_capacity(work.len());
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
                            .map(|(model, refr)| self.load_static_surfaces(model, refr))
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

    fn load_static_surfaces(
        &self,
        model: &str,
        refr: &Refr,
    ) -> Result<Vec<SurfaceArrays>, DataSourceError> {
        let _span = soe_trace::Span::new("host.static", "host");
        let path = format!("meshes/{model}");
        let bytes = self.vfs.read(&path)?;
        let file = soe_nif::parse(&bytes)?;
        let meshes = file.meshes()?;
        let refr_transform = placement_transform(refr);
        let mut out = Vec::with_capacity(meshes.len());
        for shape in &meshes {
            let _surface = soe_trace::Span::new("host.build_surface", "host");
            let combined = refr_transform.then(&shape.transform);
            let data = mesh_data_from(shape, &combined);
            out.push(build_surface(&data, MeshOptions::default())?);
        }
        Ok(out)
    }
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
