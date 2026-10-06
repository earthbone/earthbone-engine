// SPDX-License-Identifier: GPL-3.0-or-later

//! Cell-local loading attribution, following godotwind's model timing table.
//! Captures are shared with joined workers and independent of event tracing.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// Stages of cell preparation and host submission. Times are inclusive work
/// summed across workers, rather than fractions of elapsed wall time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum LoadStage {
    ArchiveIo,
    NifParse,
    TextureDecode,
    TextureUpload,
    CollisionBuild,
    Navmesh,
}

impl LoadStage {
    /// Stable report slots, including stages with no work in a load.
    pub const ALL: [Self; 6] = [
        Self::ArchiveIo,
        Self::NifParse,
        Self::TextureDecode,
        Self::TextureUpload,
        Self::CollisionBuild,
        Self::Navmesh,
    ];

    /// Machine-readable stage name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ArchiveIo => "archive_io",
            Self::NifParse => "nif_parse",
            Self::TextureDecode => "texture_decode",
            Self::TextureUpload => "texture_upload",
            Self::CollisionBuild => "collision_build",
            Self::Navmesh => "navmesh",
        }
    }
}

/// One cell's accumulated work. Archive keys name the winning source, and
/// asset keys use canonical VFS paths. Shared textures count once per cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub stage_ns: [u64; 6],
    pub archive_ns: BTreeMap<String, u64>,
    pub asset_ns: BTreeMap<String, u64>,
    pub texture_bytes: BTreeMap<String, usize>,
}

impl LoadReport {
    /// Sum of the distinct texture upload payloads used by this cell.
    pub fn texture_total(&self) -> usize {
        self.texture_bytes.values().copied().sum()
    }

    /// Highest accumulated asset cost, with lexical tie breaking.
    pub fn slowest_asset(&self) -> Option<(&str, u64)> {
        self.asset_ns
            .iter()
            .max_by(|(a, av), (b, bv)| av.cmp(bv).then_with(|| b.cmp(a)))
            .map(|(name, ns)| (name.as_str(), *ns))
    }

    /// Text suitable for a streaming diagnostic or a hitch capture.
    pub fn diagnostic(&self) -> String {
        use std::fmt::Write as _;
        let mut text = String::new();
        for stage in LoadStage::ALL {
            let _ = writeln!(
                text,
                "{} {} ns",
                stage.name(),
                self.stage_ns[stage as usize]
            );
        }
        for (archive, ns) in &self.archive_ns {
            let _ = writeln!(text, "archive {archive} {ns} ns");
        }
        if let Some((asset, ns)) = self.slowest_asset() {
            let _ = writeln!(text, "slowest_asset {asset} {ns} ns");
        }
        let _ = writeln!(text, "texture_bytes {}", self.texture_total());
        text
    }
}

/// Shared recorder retained with a loaded cell through preparation and upload.
#[derive(Debug, Clone, Default)]
pub struct LoadRecorder(Arc<Mutex<LoadReport>>);

thread_local! {
    static CURRENT: RefCell<Option<LoadRecorder>> = const { RefCell::new(None) };
}

impl LoadRecorder {
    /// Current capture, for explicit propagation into joined workers.
    pub fn current() -> Option<Self> {
        CURRENT.with(|current| current.borrow().clone())
    }

    /// Enter this cell's capture, restoring the previous one on drop or unwind.
    pub fn enter(&self) -> LoadCapture {
        LoadCapture {
            previous: CURRENT.with(|current| current.replace(Some(self.clone()))),
            _thread: PhantomData,
        }
    }

    /// Snapshot without clearing the retained report.
    pub fn report(&self) -> LoadReport {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Replace the texture inventory after all geometry, including LAND, is ready.
    pub fn set_textures(&self, textures: BTreeMap<String, usize>) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .texture_bytes = textures;
    }

    /// Include already completed game preparation when the host attaches a cell.
    pub fn add_duration(&self, stage: LoadStage, ns: u64) {
        let mut report = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        report.stage_ns[stage as usize] = report.stage_ns[stage as usize].saturating_add(ns);
    }
}

/// Thread-bound capture restoration guard.
pub struct LoadCapture {
    previous: Option<LoadRecorder>,
    _thread: PhantomData<Rc<()>>,
}

impl Drop for LoadCapture {
    fn drop(&mut self) {
        CURRENT.with(|current| current.replace(self.previous.take()));
    }
}

/// Measures an operation only while a cell capture is active. Archive read
/// time includes decompression and lock waits, matching the VFS read contract.
pub struct LoadMeasure {
    capture: Option<(LoadRecorder, Instant, LoadStage, String, Option<String>)>,
}

impl LoadMeasure {
    /// Measure a named asset operation.
    pub fn new(stage: LoadStage, asset: &str) -> Self {
        Self {
            capture: LoadRecorder::current()
                .map(|recorder| (recorder, Instant::now(), stage, asset.to_owned(), None)),
        }
    }

    /// Measure I/O from the winning archive or loose file source.
    pub fn archive(asset: &str, source: &str) -> Self {
        let mut measure = Self::new(LoadStage::ArchiveIo, asset);
        if let Some(capture) = &mut measure.capture {
            capture.4 = Some(source.to_owned());
        }
        measure
    }
}

impl Drop for LoadMeasure {
    fn drop(&mut self) {
        let Some((recorder, start, stage, asset, archive)) = self.capture.take() else {
            return;
        };
        let ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let mut report = recorder.0.lock().unwrap_or_else(PoisonError::into_inner);
        report.stage_ns[stage as usize] = report.stage_ns[stage as usize].saturating_add(ns);
        if !asset.is_empty() {
            let total = report.asset_ns.entry(asset).or_default();
            *total = total.saturating_add(ns);
        }
        if let Some(archive) = archive {
            let total = report.archive_ns.entry(archive).or_default();
            *total = total.saturating_add(ns);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_20_stages_archives_assets_and_worker_capture_are_cell_local() {
        let recorder = LoadRecorder::default();
        {
            let _capture = recorder.enter();
            std::thread::scope(|scope| {
                for stage in LoadStage::ALL {
                    let recorder = LoadRecorder::current().unwrap();
                    scope.spawn(move || {
                        let _capture = recorder.enter();
                        let _measure = LoadMeasure::new(stage, "meshes/chair.nif");
                    });
                }
            });
            let _io = LoadMeasure::archive("meshes/chair.nif", "Patch.bsa");
        }
        assert!(LoadRecorder::current().is_none());
        let report = recorder.report();
        assert!(report.stage_ns.iter().all(|ns| *ns > 0));
        assert!(report.archive_ns["Patch.bsa"] > 0);
        assert_eq!(report.slowest_asset().unwrap().0, "meshes/chair.nif");
        assert!(report
            .diagnostic()
            .contains("slowest_asset meshes/chair.nif"));
        assert_eq!(LoadRecorder::default().report(), LoadReport::default());
    }

    #[test]
    fn prf_20_texture_inventory_sums_distinct_paths_and_asset_ties_are_stable() {
        let mut report = LoadReport::default();
        report
            .texture_bytes
            .extend([("textures/a.dds".into(), 8), ("textures/b.dds".into(), 16)]);
        report.asset_ns.extend([("b".into(), 5), ("a".into(), 5)]);
        assert_eq!(report.texture_total(), 24);
        assert_eq!(report.slowest_asset(), Some(("a", 5)));
        for stage in LoadStage::ALL {
            assert!(report.diagnostic().contains(stage.name()));
        }
    }
}
