// SPDX-License-Identifier: GPL-3.0-or-later

//! DDS upload payloads and a per-source single-load cache. Uses the VFS's
//! path normalization and Godot's complete mip-chain requirement.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use soe_bsa::Vfs;
use soe_dds::{Dds, Image as DdsImage};

use super::{DataSourceError, SurfaceTextures};
use crate::mesh::MaterialDesc;

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
    /// Complete DDS cubemap faces in +X, -X, +Y, -Y, +Z, -Z order.
    Cubemap(Box<[TextureData; 6]>),
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
            TextureData::Cubemap(faces) => faces[0].width(),
            TextureData::Compressed { width, .. } => *width,
            TextureData::Rgba8(image) => image.width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            TextureData::Cubemap(faces) => faces[0].height(),
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
    if dds.header().is_cubemap() {
        if dds.layer_count() != 6 || dds.width() != dds.height() || dds.header().depth() != 1 {
            return Err(soe_dds::Error::BadHeader(
                "environment cubemap requires six square 2D faces",
            )
            .into());
        }
        let faces: Vec<_> = (0..6)
            .map(|layer| texture_layer(&dds, layer))
            .collect::<Result<_, _>>()?;
        let faces = faces
            .try_into()
            .map_err(|_| soe_dds::Error::BadHeader("cubemap face count"))?;
        return Ok(TextureData::Cubemap(Box::new(faces)));
    }
    texture_layer(&dds, 0)
}

fn texture_layer(dds: &Dds<'_>, layer: u32) -> Result<TextureData, DataSourceError> {
    let format = match dds.format() {
        soe_dds::Format::Bc1 => Some(BlockFormat::Bc1),
        soe_dds::Format::Bc2 => Some(BlockFormat::Bc2),
        soe_dds::Format::Bc3 => Some(BlockFormat::Bc3),
        soe_dds::Format::Bc4 { signed: false } => Some(BlockFormat::Bc4),
        soe_dds::Format::Bc5 { signed: false } => Some(BlockFormat::Bc5),
        soe_dds::Format::Bc7 => Some(BlockFormat::Bc7),
        _ => None,
    };
    let flat = dds.level(layer, 0).map(|l| l.depth == 1).unwrap_or(false);
    let Some(format) = format.filter(|_| flat) else {
        return Ok(TextureData::Rgba8(dds.decode(layer, 0)?));
    };
    let stored = dds.mip_count().max(1);
    let mut payload = Vec::new();
    for mip in 0..stored {
        payload.extend_from_slice(dds.level_bytes(layer, mip)?);
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
        mip0_len: dds.level(layer, 0)?.len,
        bytes: payload,
    })
}

/// Decoded textures keyed by canonical VFS path, owned by the `DataSource` and
/// shared by every stream call and every worker thread, so a texture used by
/// forty statics across nine cells decodes once for the source's lifetime.
/// The counters run since open. A stream call reads them before and after
/// to fill its own `CellStats`, which is exact as long as stream calls on
/// one source do not overlap. Concurrent cell calls share these totals.
#[derive(Default)]
pub(super) struct TextureCache {
    /// One slot per canonical path. The map lock is held only to find or
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
pub(super) struct TextureCounters {
    pub(super) decoded: usize,
    pub(super) failures: usize,
    pub(super) decode_ns: u64,
    pub(super) missing: usize,
}

impl TextureCache {
    pub(super) fn resolve(&self, vfs: &Vfs, material: &MaterialDesc) -> SurfaceTextures {
        SurfaceTextures {
            environment: material
                .environment_texture()
                .and_then(|p| self.get_or_decode(vfs, p)),
            environment_mask: material
                .environment_texture()
                .and(material.environment_mask.as_deref())
                .and_then(|p| self.get_or_decode(vfs, p)),
            diffuse: material
                .diffuse
                .as_deref()
                .and_then(|p| self.get_or_decode(vfs, p)),
            normal: material
                .normal
                .as_deref()
                .and_then(|p| self.get_or_decode(vfs, p)),
            glow: material
                .glow_texture()
                .and_then(|p| self.get_or_decode(vfs, p)),
        }
    }

    fn get_or_decode(&self, vfs: &Vfs, path: &str) -> Option<Arc<TextureData>> {
        self.get_or_load(path, |canonical| self.read(vfs, canonical))
    }

    fn get_or_load(
        &self,
        path: &str,
        load: impl FnOnce(&str) -> Option<Arc<TextureData>>,
    ) -> Option<Arc<TextureData>> {
        let path = soe_bsa::normalize(path);
        let slot = self
            .textures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(path.clone())
            .or_default()
            .clone();
        slot.get_or_init(|| load(&path)).clone()
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

    pub(super) fn counters(&self) -> TextureCounters {
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

    pub(super) fn missing_since(&self, count: usize) -> Vec<String> {
        let mut paths = self
            .missing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(count..)
            .map(<[String]>::to_vec)
            .unwrap_or_default();
        paths.sort_unstable();
        paths
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    use super::{BlockFormat, TextureCache, TextureData};

    #[test]
    fn concurrent_path_aliases_share_one_successful_load() {
        let cache = TextureCache::default();
        let loads = AtomicUsize::new(0);
        let barrier = Barrier::new(12);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..12)
                .map(|i| {
                    let (cache, loads, barrier) = (&cache, &loads, &barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        let path = [
                            "Textures\\Stone.DDS",
                            "/Data/textures/stone.dds",
                            "textures/stone.dds",
                        ][i % 3];
                        cache
                            .get_or_load(path, |canonical| {
                                assert_eq!(canonical, "textures/stone.dds");
                                loads.fetch_add(1, Ordering::SeqCst);
                                Some(Arc::new(TextureData::Compressed {
                                    width: 4,
                                    height: 4,
                                    format: BlockFormat::Bc1,
                                    mips: 1,
                                    mip0_len: 8,
                                    bytes: vec![0; 8],
                                }))
                            })
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(loads.load(Ordering::SeqCst), 1, "aliases must load once");
        assert!(
            results.iter().all(|r| Arc::ptr_eq(&results[0], r)),
            "every caller shares the same payload"
        );
    }

    #[test]
    fn concurrent_failed_loads_are_cached_too() {
        let cache = TextureCache::default();
        let loads = AtomicUsize::new(0);
        let barrier = Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let (cache, loads, barrier) = (&cache, &loads, &barrier);
                scope.spawn(move || {
                    barrier.wait();
                    assert!(cache
                        .get_or_load("missing.dds", |_| {
                            loads.fetch_add(1, Ordering::SeqCst);
                            None
                        })
                        .is_none());
                });
            }
        });
        assert_eq!(loads.load(Ordering::SeqCst), 1, "a failure is cached");
    }

    #[test]
    fn missing_paths_are_reported_in_canonical_order() {
        let cache = TextureCache::default();
        let vfs = soe_bsa::Vfs::new();
        cache.get_or_decode(&vfs, "Textures\\Z.DDS");
        cache.get_or_decode(&vfs, "/Data/textures/a.dds");
        cache.get_or_decode(&vfs, "textures/z.dds");
        assert_eq!(cache.missing_since(0), ["textures/a.dds", "textures/z.dds"]);
    }
}
