// SPDX-License-Identifier: GPL-3.0-or-later

//! One speculative interior preparation, following OpenMW's nearby teleport
//! door preload policy. Only immutable host inputs cross the worker boundary.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use soe_esm::GlobalFormId;

use crate::data_source::{DataSource, DataSourceError, LoadedLocation, TextureData};

pub const APPROACH_RADIUS: f32 = 500.0;
pub const SELECTION_INTERVAL: f64 = 0.25;
pub const MAX_REFERENCES: usize = 4096;
pub const MAX_PAYLOAD_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_COLLISION_PRIMITIVES: usize = 100_000;

type Prepared = Result<Option<LoadedLocation>, DataSourceError>;

struct Job {
    destination: GlobalFormId,
    cancel: Arc<AtomicBool>,
    worker: JoinHandle<Prepared>,
}

/// A single outstanding job or completed destination, never an unbounded queue.
#[derive(Default)]
pub struct InteriorPrefetch {
    wanted: Option<GlobalFormId>,
    job: Option<Job>,
    ready: Option<(GlobalFormId, LoadedLocation)>,
    pub started: u64,
    pub hits: u64,
    pub rejected: u64,
    pub payload_bytes: usize,
}

impl InteriorPrefetch {
    /// Forget a stale destination without joining an unfinished worker.
    pub fn invalidate(&mut self) {
        self.wanted = None;
        self.ready = None;
        self.payload_bytes = 0;
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Collect finished work only. An unfinished decode never blocks a frame.
    pub fn poll(&mut self) {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| job.worker.is_finished())
        {
            return;
        }
        let Some(job) = self.job.take() else { return };
        let cancelled = job.cancel.load(Ordering::Relaxed);
        match job.worker.join() {
            Ok(Ok(Some(location))) if !cancelled && self.wanted == Some(job.destination) => {
                self.payload_bytes = payload_bytes(&location);
                self.ready = Some((job.destination, location));
            }
            Ok(Ok(_)) if cancelled => {}
            _ => self.rejected += 1,
        }
    }

    /// Schedule one eligible interior, with an authored-reference admission cap.
    pub fn request(&mut self, source: Arc<DataSource>, destination: GlobalFormId) {
        if self.wanted == Some(destination) {
            return;
        }
        self.invalidate();
        if self.job.is_some() {
            return;
        }
        self.wanted = Some(destination);
        if !source.cell_index().is_interior(destination)
            || source.cell_index().references(destination).len() > MAX_REFERENCES
        {
            self.rejected += 1;
            return;
        }
        self.start(destination, move || {
            let source = source.speculative_view();
            let location = source.load_location(destination, [0.0; 3], 0)?;
            if !admitted(&location) {
                eprintln!(
                    "interior_prefetch: rejected {destination} payload={} primitives={}",
                    payload_bytes(&location),
                    location.collision.stats.primitives
                );
                return Ok(None);
            }
            Ok(Some(location))
        });
    }

    fn start(
        &mut self,
        destination: GlobalFormId,
        load: impl FnOnce() -> Prepared + Send + 'static,
    ) {
        if self.job.is_some() || self.ready.is_some() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        match std::thread::Builder::new()
            .name("interior-prefetch".into())
            .spawn(move || {
                let prepared = load();
                if worker_cancel.load(Ordering::Relaxed) {
                    Ok(None)
                } else {
                    prepared
                }
            }) {
            Ok(worker) => {
                self.wanted = Some(destination);
                self.started += 1;
                self.job = Some(Job {
                    destination,
                    cancel,
                    worker,
                });
            }
            Err(_) => self.rejected += 1,
        }
    }

    /// Consume a completed destination on the normal gameplay transition path.
    pub fn take(&mut self, destination: GlobalFormId) -> Option<LoadedLocation> {
        self.poll();
        if self
            .ready
            .as_ref()
            .is_some_and(|(cell, _)| *cell == destination)
        {
            self.hits += 1;
            self.payload_bytes = 0;
            self.ready.take().map(|(_, location)| location)
        } else {
            None
        }
    }

    pub fn ready_cell(&self) -> Option<GlobalFormId> {
        self.ready.as_ref().map(|(cell, _)| *cell)
    }

    pub fn pending(&self) -> bool {
        self.job.is_some()
    }
}

impl Drop for InteriorPrefetch {
    fn drop(&mut self) {
        self.invalidate();
        // The shared source must not outlive shutdown through a detached worker.
        if let Some(job) = self.job.take() {
            let _ = job.worker.join();
        }
    }
}

fn admitted(location: &LoadedLocation) -> bool {
    location.cells.len() == 1
        && location.collision.stats.primitives <= MAX_COLLISION_PRIMITIVES
        && payload_bytes(location) <= MAX_PAYLOAD_BYTES
}

fn payload_bytes(location: &LoadedLocation) -> usize {
    let mut bytes = 0usize;
    let mut seen = HashSet::new();
    for (_, cell) in &location.cells {
        for surface in &cell.surfaces {
            let arrays = &surface.arrays;
            for size in [
                std::mem::size_of_val(arrays.positions.as_slice()),
                std::mem::size_of_val(arrays.normals.as_slice()),
                std::mem::size_of_val(arrays.tangents.as_slice()),
                std::mem::size_of_val(arrays.uvs.as_slice()),
                std::mem::size_of_val(arrays.indices.as_slice()),
                std::mem::size_of_val(arrays.bones.as_slice()),
                std::mem::size_of_val(arrays.weights.as_slice()),
            ] {
                bytes = bytes.saturating_add(size);
            }
            for texture in [
                &surface.textures.diffuse,
                &surface.textures.normal,
                &surface.textures.environment,
                &surface.textures.environment_mask,
                &surface.textures.glow,
            ]
            .into_iter()
            .flatten()
            {
                if seen.insert(Arc::as_ptr(texture)) {
                    bytes = bytes.saturating_add(texture_bytes(texture.as_ref()));
                }
            }
        }
    }
    bytes
}

fn texture_bytes(texture: &TextureData) -> usize {
    match texture {
        TextureData::Cubemap(faces) => faces.iter().map(texture_bytes).sum(),
        TextureData::Compressed { bytes, .. } => bytes.len(),
        TextureData::Rgba8(image) => image.rgba.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{CellCollision, CollisionStats};
    use crate::data_source::LoadedCell;

    const CELL: GlobalFormId = GlobalFormId { file: 0, object: 1 };

    fn location() -> LoadedLocation {
        LoadedLocation {
            cells: vec![(
                CELL,
                LoadedCell {
                    stats: crate::data_source::CellStats::default(),
                    surfaces: Vec::new(),
                },
            )],
            collision_cells: std::collections::BTreeMap::new(),
            collision: CellCollision {
                colliders: Vec::new(),
                world: soe_physics::SimpleWorld::new(),
                stats: CollisionStats::default(),
                references: std::collections::HashMap::default(),
            },
        }
    }

    fn finish(prefetch: &mut InteriorPrefetch) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while prefetch.pending() {
            assert!(std::time::Instant::now() < until, "worker did not finish");
            prefetch.poll();
            std::thread::yield_now();
        }
    }

    #[test]
    fn unfinished_worker_never_blocks_poll_or_stale_transition() {
        let (release, wait) = std::sync::mpsc::channel();
        let mut prefetch = InteriorPrefetch::default();
        prefetch.start(CELL, move || {
            wait.recv().unwrap();
            Ok(Some(location()))
        });
        prefetch.start(CELL, || panic!("a second worker must not start"));
        assert_eq!(prefetch.started, 1);
        prefetch.poll();
        assert!(prefetch.pending());
        prefetch.invalidate();
        assert!(prefetch.take(CELL).is_none());
        release.send(()).unwrap();
        finish(&mut prefetch);
        assert!(
            prefetch.take(CELL).is_none(),
            "stale worker published a destination"
        );
    }

    #[test]
    fn completed_destination_is_consumed_once_and_only_for_matching_cell() {
        let mut prefetch = InteriorPrefetch::default();
        prefetch.start(CELL, || Ok(Some(location())));
        finish(&mut prefetch);
        assert!(prefetch.take(GlobalFormId { file: 0, object: 2 }).is_none());
        assert!(prefetch.take(CELL).is_some());
        assert!(prefetch.take(CELL).is_none());
        assert_eq!(prefetch.hits, 1);
    }

    #[test]
    fn payload_and_collision_limits_reject_oversized_destinations() {
        let mut data = location();
        data.collision.stats.primitives = MAX_COLLISION_PRIMITIVES;
        assert!(admitted(&data));
        data.collision.stats.primitives += 1;
        assert!(!admitted(&data));
        data.collision.stats.primitives = 0;
        let texture = Arc::new(TextureData::Compressed {
            width: 1,
            height: 1,
            format: crate::data_source::BlockFormat::Bc1,
            mips: 1,
            mip0_len: 8,
            bytes: vec![0; MAX_PAYLOAD_BYTES + 1],
        });
        let arrays = crate::mesh::build_surface(
            &crate::mesh::MeshData {
                positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                indices: vec![0, 1, 2],
                ..Default::default()
            },
            crate::mesh::MeshOptions::default(),
        )
        .unwrap();
        data.cells[0]
            .1
            .surfaces
            .push(crate::data_source::LoadedSurface {
                reference: CELL,
                arrays,
                placement: soe_physics::Iso::IDENTITY,
                textures: crate::data_source::SurfaceTextures {
                    diffuse: Some(Arc::clone(&texture)),
                    normal: Some(texture),
                    environment: None,
                    environment_mask: None,
                    glow: None,
                },
                instance_transform: None,
                mesh_identity: None,
            });
        assert!(
            payload_bytes(&data) < MAX_PAYLOAD_BYTES + 1024,
            "shared texture counted twice"
        );
        assert!(!admitted(&data));
    }

    #[test]
    fn panicked_worker_is_collected_and_shutdown_joins_an_active_job() {
        let mut prefetch = InteriorPrefetch::default();
        prefetch.start(CELL, || panic!("injected worker failure"));
        finish(&mut prefetch);
        assert_eq!(prefetch.rejected, 1);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        prefetch.start(CELL, move || {
            worker_completed.store(true, Ordering::SeqCst);
            Ok(Some(location()))
        });
        drop(prefetch);
        assert!(completed.load(Ordering::SeqCst));
    }
    #[test]
    fn installed_bannered_mare_prefetch_admission() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = DataSource::open_with_threads(std::path::Path::new(&data), 1).unwrap();
        let destination = source.resolve_cell("WhiterunBanneredMare").unwrap();
        let prepared = source.load_location(destination, [0.0; 3], 0).unwrap();
        eprintln!(
            "PREFETCH ADMISSION references={} bytes={} primitives={} admitted={}",
            source.cell_index().references(destination).len(),
            payload_bytes(&prepared),
            prepared.collision.stats.primitives,
            admitted(&prepared)
        );
        assert!(
            admitted(&prepared),
            "installed acceptance destination exceeds admission limits"
        );
    }
}
