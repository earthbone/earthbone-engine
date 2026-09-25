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
// Measured against installed Special Edition data on 2026-09-24 (HST-07
// regression), decoded texture and mesh payload per interior:
// WhiterunBanneredMare 186.7 MiB, RiftenBeeandBarb 195.6 MiB,
// SolitudeBluePalace 230.7 MiB, WhiterunJorrvaskr 269.9 MiB,
// WhiterunDragonsreach 295.3 MiB. The previous 128 MiB cap rejected every
// one of them, so no prepared interior could ever become ready.
// 384 MiB keeps a real, enforced ceiling while clearing all five with
// headroom above the largest.
pub const MAX_PAYLOAD_BYTES: usize = 384 * 1024 * 1024;
pub const MAX_COLLISION_PRIMITIVES: usize = 100_000;

type Prepared = Result<Option<LoadedLocation>, DataSourceError>;
type Loader = Box<dyn FnOnce() -> Prepared + Send + 'static>;

/// What a finished worker hands back, so a cancelled job is never counted
/// as a budget rejection.
enum Outcome {
    Ready(Box<LoadedLocation>),
    Rejected,
    Cancelled,
}

struct Job {
    destination: GlobalFormId,
    cancel: Arc<AtomicBool>,
    worker: JoinHandle<Result<Outcome, DataSourceError>>,
}

/// A single outstanding job or completed destination, never an unbounded queue.
#[derive(Default)]
pub struct InteriorPrefetch {
    wanted: Option<GlobalFormId>,
    job: Option<Job>,
    // The newest selection made while a job was still running. At most one is
    // held, it replaces any earlier one, and it starts once that job is
    // collected, so a changed selection is delayed rather than dropped.
    queued: Option<(GlobalFormId, Loader)>,
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
        self.queued = None;
        self.ready = None;
        self.payload_bytes = 0;
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Collect finished work only. An unfinished decode never blocks a frame.
    /// A selection queued behind the collected job starts here.
    pub fn poll(&mut self) {
        if !self
            .job
            .as_ref()
            .is_some_and(|job| job.worker.is_finished())
        {
            return;
        }
        let Some(job) = self.job.take() else { return };
        match job.worker.join() {
            Ok(Ok(Outcome::Ready(location))) if self.wanted == Some(job.destination) => {
                self.payload_bytes = payload_bytes(&location);
                self.ready = Some((job.destination, *location));
                self.queued = None;
            }
            Ok(Ok(Outcome::Ready(_) | Outcome::Cancelled)) => {}
            Ok(Ok(Outcome::Rejected) | Err(_)) | Err(_) => {
                self.rejected += 1;
                // A destination that just failed is not retried until the
                // selection changes.
                if self
                    .queued
                    .as_ref()
                    .is_some_and(|(queued, _)| *queued == job.destination)
                {
                    self.queued = None;
                }
            }
        }
        if let Some((destination, load)) = self.queued.take() {
            if self.wanted == Some(destination) {
                self.start(destination, load);
            }
        }
    }

    /// Schedule one eligible interior, with an authored-reference admission cap.
    /// While a job runs, the newest selection waits for it instead of spawning
    /// a second worker.
    pub fn request(&mut self, source: Arc<DataSource>, destination: GlobalFormId) {
        if self.wanted == Some(destination) {
            return;
        }
        self.invalidate();
        self.wanted = Some(destination);
        if !source.cell_index().is_interior(destination)
            || source.cell_index().references(destination).len() > MAX_REFERENCES
        {
            self.rejected += 1;
            return;
        }
        self.schedule(
            destination,
            Box::new(move || {
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
            }),
        );
    }

    /// Start `load` now, or hold it as the single queued selection while a
    /// job runs.
    fn schedule(&mut self, destination: GlobalFormId, load: Loader) {
        if let Some(job) = &self.job {
            // Returning to the door whose job is still running keeps that
            // job. If the worker already saw the cancel, the queue restarts it.
            if job.destination == destination {
                job.cancel.store(false, Ordering::Relaxed);
            }
            self.queued = Some((destination, load));
            return;
        }
        self.start(destination, load);
    }

    fn start(&mut self, destination: GlobalFormId, load: Loader) {
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
                    Ok(Outcome::Cancelled)
                } else {
                    prepared.map(|location| {
                        location.map_or(Outcome::Rejected, |l| Outcome::Ready(Box::new(l)))
                    })
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
        prefetch.start(
            CELL,
            Box::new(move || {
                wait.recv().unwrap();
                Ok(Some(location()))
            }),
        );
        prefetch.start(CELL, Box::new(|| panic!("a second worker must not start")));
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
        prefetch.start(CELL, Box::new(|| Ok(Some(location()))));
        finish(&mut prefetch);
        assert!(prefetch.take(GlobalFormId { file: 0, object: 2 }).is_none());
        assert!(prefetch.take(CELL).is_some());
        assert!(prefetch.take(CELL).is_none());
        assert_eq!(prefetch.hits, 1);
    }

    #[test]
    fn hst_07_bounded_budget_rejects_oversized_destinations() {
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
        prefetch.start(CELL, Box::new(|| panic!("injected worker failure")));
        finish(&mut prefetch);
        assert_eq!(prefetch.rejected, 1);
        let completed = Arc::new(AtomicBool::new(false));
        let worker_completed = Arc::clone(&completed);
        prefetch.start(
            CELL,
            Box::new(move || {
                worker_completed.store(true, Ordering::SeqCst);
                Ok(Some(location()))
            }),
        );
        drop(prefetch);
        assert!(completed.load(Ordering::SeqCst));
    }

    /// What `request` does after its reference-count admission.
    fn select(prefetch: &mut InteriorPrefetch, destination: GlobalFormId, load: Loader) {
        if prefetch.wanted == Some(destination) {
            return;
        }
        prefetch.invalidate();
        prefetch.wanted = Some(destination);
        prefetch.schedule(destination, load);
    }

    fn gated(
        cell: GlobalFormId,
        wait: std::sync::mpsc::Receiver<()>,
    ) -> impl FnOnce() -> Prepared + Send + 'static {
        move || {
            let _ = wait.recv();
            let mut prepared = location();
            prepared.cells[0].0 = cell;
            Ok(Some(prepared))
        }
    }

    #[test]
    fn hst_07_a_changed_selection_waits_for_the_running_job_and_then_starts() {
        const OTHER: GlobalFormId = GlobalFormId { file: 0, object: 2 };
        let (release, wait) = std::sync::mpsc::channel();
        let mut prefetch = InteriorPrefetch::default();
        select(&mut prefetch, CELL, Box::new(gated(CELL, wait)));
        select(&mut prefetch, OTHER, Box::new(|| Ok(Some(location()))));
        assert_eq!(prefetch.started, 1, "a second worker ran beside the first");
        release.send(()).unwrap();
        finish(&mut prefetch);
        assert_eq!(prefetch.started, 2, "the newer selection was dropped");
        assert_eq!(prefetch.ready_cell(), Some(OTHER));
        assert!(prefetch.take(CELL).is_none(), "the stale job was published");
        assert_eq!(prefetch.rejected, 0, "a cancel was counted as a rejection");
    }

    #[test]
    fn hst_07_returning_to_the_running_door_keeps_its_job() {
        let (release, wait) = std::sync::mpsc::channel();
        let mut prefetch = InteriorPrefetch::default();
        select(&mut prefetch, CELL, Box::new(gated(CELL, wait)));
        prefetch.invalidate();
        select(&mut prefetch, CELL, Box::new(|| Ok(Some(location()))));
        release.send(()).unwrap();
        finish(&mut prefetch);
        assert!(prefetch.started <= 2);
        assert!(
            prefetch.take(CELL).is_some(),
            "the returned-to door was lost"
        );
    }

    #[test]
    fn hst_07_a_rejected_destination_is_not_retried_for_the_same_selection() {
        let mut prefetch = InteriorPrefetch::default();
        select(&mut prefetch, CELL, Box::new(|| Ok(None)));
        finish(&mut prefetch);
        select(&mut prefetch, CELL, Box::new(|| panic!("retried")));
        assert_eq!((prefetch.started, prefetch.rejected), (1, 1));
        assert!(!prefetch.pending());
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
