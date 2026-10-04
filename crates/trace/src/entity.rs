// SPDX-License-Identifier: GPL-3.0-or-later

//! Optional entity attribution for PRF-13. Bounded rolling samples follow
//! godotwind's performance_profiler.gd. Inclusive and self time follow the
//! local Godot profiler manual. Export uses this crate's Chrome event writer.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::{epoch, json_escape, thread_id, Operation, WorkCounts};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DATA: Mutex<Data> = Mutex::new(Data {
    events: VecDeque::new(),
    totals: BTreeMap::new(),
});

/// The measured entity and its execution phase. Plugin means the defining
/// file for forms and the winning asset provider for loaded scripts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entity {
    pub kind: &'static str,
    pub id: String,
    pub plugin: String,
    pub phase: &'static str,
}

impl Entity {
    /// Construct an entity inside a lazy scope factory.
    #[must_use]
    pub fn new(
        kind: &'static str,
        id: impl Into<String>,
        plugin: impl Into<String>,
        phase: &'static str,
    ) -> Self {
        Self {
            kind,
            id: id.into(),
            plugin: plugin.into(),
            phase,
        }
    }
}

/// Counts and nanosecond timings for an entity, phase or plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntityStats {
    pub calls: u64,
    pub total_ns: u64,
    pub self_ns: u64,
    pub wait_ns: u64,
    pub waits: u64,
    pub work: WorkCounts,
}

impl EntityStats {
    fn add(&mut self, other: &Self) {
        self.calls = self.calls.saturating_add(other.calls);
        self.total_ns = self.total_ns.saturating_add(other.total_ns);
        self.self_ns = self.self_ns.saturating_add(other.self_ns);
        self.wait_ns = self.wait_ns.saturating_add(other.wait_ns);
        self.waits = self.waits.saturating_add(other.waits);
        for (count, add) in self.work.0.iter_mut().zip(other.work.0) {
            *count = count.saturating_add(add);
        }
    }
}

struct Sample {
    entity: Entity,
    stats: EntityStats,
    start_ns: u64,
    end: Instant,
    thread: u64,
}

struct Data {
    events: VecDeque<Sample>,
    totals: BTreeMap<Entity, EntityStats>,
}

struct Active {
    entity: Entity,
    start: Instant,
    child_ns: u64,
    stats: EntityStats,
}

thread_local! {
    static STACK: RefCell<Vec<Active>> = const { RefCell::new(Vec::new()) };
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn record(entity: Entity, stats: EntityStats, start: Instant, end: Instant) {
    if let Ok(mut data) = DATA.lock() {
        data.totals.entry(entity.clone()).or_default().add(&stats);
        while data.events.len() >= crate::MAX_EVENTS {
            data.events.pop_front();
        }
        data.events.push_back(Sample {
            entity,
            stats,
            start_ns: nanos(start.saturating_duration_since(epoch())),
            end,
            thread: thread_id(),
        });
    }
}

/// A main-thread scope. Disabled scopes perform one relaxed atomic read and
/// never invoke their factory. Nested work counts belong to the innermost
/// entity. Inclusive time includes children, self time excludes them.
#[must_use]
pub struct EntityScope {
    active: bool,
    thread: PhantomData<Rc<()>>,
}

impl EntityScope {
    /// Capture a scope lazily. Calls can be zero for subsequent instructions
    /// in a function, so instruction counts do not become function calls.
    pub fn new(entity: impl FnOnce() -> Entity, calls: u64) -> Self {
        let active = Profile::enabled();
        if active {
            let entity = entity();
            let _ = epoch();
            STACK.with(|stack| {
                stack.borrow_mut().push(Active {
                    entity,
                    start: Instant::now(),
                    child_ns: 0,
                    stats: EntityStats {
                        calls,
                        ..EntityStats::default()
                    },
                });
            });
        }
        Self {
            active,
            thread: PhantomData,
        }
    }
}

impl Drop for EntityScope {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let end = Instant::now();
        let sample = STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            let mut active = stack.pop()?;
            active.stats.total_ns = nanos(end.duration_since(active.start));
            active.stats.self_ns = active.stats.total_ns.saturating_sub(active.child_ns);
            if let Some(parent) = stack.last_mut() {
                parent.child_ns = parent.child_ns.saturating_add(active.stats.total_ns);
            }
            Some(active)
        });
        if let Some(sample) = sample {
            record(sample.entity, sample.stats, sample.start, end);
        }
    }
}

pub(crate) fn count(operation: Operation, amount: u64) {
    if Profile::enabled() {
        STACK.with(|stack| {
            if let Some(active) = stack.borrow_mut().last_mut() {
                active.stats.work.0[operation as usize] += amount;
            }
        });
    }
}

/// Optional capture shared by the walkthrough and headless engine. All data
/// is diagnostic and excluded from saves and gameplay hashes.
pub struct Profile;

/// A key's wall-clock latency until its applied state is presented.
pub struct InputLatency {
    entity: Entity,
    start: Instant,
}

impl InputLatency {
    /// Complete at the frame-post-draw boundary after applying the input.
    pub fn presented(self) {
        let end = Instant::now();
        let ns = nanos(end.duration_since(self.start));
        record(
            self.entity,
            EntityStats {
                calls: 1,
                total_ns: ns,
                self_ns: ns,
                ..EntityStats::default()
            },
            self.start,
            end,
        );
    }
}

impl Profile {
    /// Measure one RenderingServer instance submission and charge its work
    /// count at the same boundary, including when entity capture is off.
    pub fn render_submission<T>(submit: impl FnOnce() -> T) -> T {
        let _entity = EntityScope::new(
            || Entity::new("output", "render_instances", "<host>", "submitted"),
            1,
        );
        crate::count(Operation::RenderInstances, 1);
        submit()
    }
    /// Start input latency without running the event-name factory when off.
    #[must_use]
    pub fn input(event: impl FnOnce() -> String) -> Option<InputLatency> {
        if !Self::enabled() {
            return None;
        }
        let _ = epoch();
        Some(InputLatency {
            entity: Entity::new("input", event(), "<host>", "presentation_latency"),
            start: Instant::now(),
        })
    }
    /// Begin capture. Clear explicitly when starting a new session.
    pub fn enable() {
        ENABLED.store(true, Ordering::Relaxed);
    }
    /// End capture, retaining its reports.
    pub fn disable() {
        ENABLED.store(false, Ordering::Relaxed);
    }
    /// Whether factories and attribution counters are active.
    #[inline]
    #[must_use]
    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }
    /// Enable headless capture when SOE_PROFILE is exactly 1.
    pub fn from_environment() {
        if std::env::var("SOE_PROFILE").is_ok_and(|value| value == "1") {
            Self::enable();
        }
    }
    /// Drop report data, leaving active scopes and the switch intact.
    pub fn clear() {
        if let Ok(mut data) = DATA.lock() {
            data.events.clear();
            data.totals.clear();
        }
    }
    /// Session totals, ordered by stable entity identity.
    #[must_use]
    pub fn summary() -> BTreeMap<Entity, EntityStats> {
        DATA.lock()
            .map(|data| data.totals.clone())
            .unwrap_or_default()
    }
    /// Only scopes completed in the preceding wall-clock second.
    #[must_use]
    pub fn last_second() -> BTreeMap<Entity, EntityStats> {
        let now = Instant::now();
        let mut totals = BTreeMap::<Entity, EntityStats>::new();
        if let Ok(data) = DATA.lock() {
            for sample in &data.events {
                if now.saturating_duration_since(sample.end) <= Duration::from_secs(1) {
                    totals
                        .entry(sample.entity.clone())
                        .or_default()
                        .add(&sample.stats);
                }
            }
        }
        totals
    }
    /// Charge elapsed scheduler wait separately from running CPU time.
    pub fn wait(entity: impl FnOnce() -> Entity, seconds: f64, calls: u64) {
        if !Self::enabled() {
            return;
        }
        let now = Instant::now();
        let wait_ns = if seconds.is_finite() {
            (seconds.max(0.0) * 1e9).min(u64::MAX as f64) as u64
        } else {
            0
        };
        record(
            entity(),
            EntityStats {
                waits: calls,
                wait_ns,
                ..EntityStats::default()
            },
            now,
            now,
        );
    }
    /// Ranking by self time. Plugins sum self time to avoid counting nested
    /// scopes twice. Counts include each phase's calls and operations.
    #[must_use]
    pub fn ranking(
        totals: &BTreeMap<Entity, EntityStats>,
        kind: &str,
    ) -> Vec<(String, EntityStats)> {
        let mut groups = BTreeMap::<String, EntityStats>::new();
        for (entity, stats) in totals {
            if kind == "plugin" || entity.kind == kind {
                let name = if kind == "plugin" {
                    &entity.plugin
                } else {
                    &entity.id
                };
                let name = if kind == "script" {
                    name.rsplit_once('.')
                        .map_or(name.as_str(), |(script, _)| script)
                } else {
                    name.as_str()
                };
                groups.entry(name.to_owned()).or_default().add(stats);
            }
        }
        let mut rows: Vec<_> = groups.into_iter().collect();
        rows.sort_by(|a, b| b.1.self_ns.cmp(&a.1.self_ns).then_with(|| a.0.cmp(&b.0)));
        rows
    }
    /// Overlay text for actors, scripts, quests and plugins over one second.
    #[must_use]
    pub fn overlay_text() -> String {
        let totals = Self::last_second();
        let mut out = String::from("F4: Entity profile, last second, self ms / calls\n");
        for kind in ["actor", "script", "quest", "plugin"] {
            let _ = writeln!(out, "{kind}:");
            for (name, stats) in Self::ranking(&totals, kind).into_iter().take(5) {
                let _ = writeln!(
                    out,
                    "{name}: {:.3} / {}",
                    stats.self_ns as f64 / 1e6,
                    stats.calls
                );
            }
        }
        out
    }
    /// Write session totals as Chrome counter events, readable by Perfetto.
    ///
    /// # Errors
    /// Returns the filesystem write error.
    pub fn export_summary(path: &Path) -> std::io::Result<()> {
        let totals = Self::summary();
        let mut out = String::from("{\"traceEvents\":[");
        for (index, (entity, stats)) in totals.iter().enumerate() {
            if index != 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"name\":\"{}.{}\",\"cat\":\"{}\",\"ph\":\"C\",\"ts\":0,\"pid\":1,\"tid\":1,\"args\":{{\"calls\":{},\"total_ns\":{},\"self_ns\":{},\"wait_ns\":{},\"waits\":{}", json_escape(&entity.id), json_escape(entity.phase), json_escape(entity.kind), stats.calls, stats.total_ns, stats.self_ns, stats.wait_ns, stats.waits);
            for op in Operation::ALL {
                let _ = write!(out, ",\"{}\":{}", op.name(), stats.work.get(op));
            }
            out.push_str("}}");
        }
        for (plugin, stats) in Self::ranking(&totals, "plugin") {
            if !totals.is_empty() || !out.ends_with('[') {
                out.push(',');
            }
            let _ = write!(out, "{{\"name\":\"{}\",\"cat\":\"plugin\",\"ph\":\"C\",\"ts\":0,\"pid\":1,\"tid\":1,\"args\":{{\"calls\":{},\"self_ns\":{}", json_escape(&plugin), stats.calls, stats.self_ns);
            for op in Operation::ALL {
                let _ = write!(out, ",\"{}\":{}", op.name(), stats.work.get(op));
            }
            out.push_str("}}");
        }
        out.push_str("]}\n");
        std::fs::write(path, out)
    }
    /// Write the retained rolling scopes with entity and plugin metadata.
    ///
    /// # Errors
    /// Returns the filesystem write error.
    pub fn export_chrome_json(path: &Path) -> std::io::Result<()> {
        let mut out = String::from("{\"traceEvents\":[");
        if let Ok(data) = DATA.lock() {
            for (index, sample) in data.events.iter().enumerate() {
                if index != 0 {
                    out.push(',');
                }
                let e = &sample.entity;
                let _ = write!(out, "{{\"name\":\"{}.{}\",\"cat\":\"{}\",\"ph\":\"X\",\"ts\":{:.3},\"dur\":{:.3},\"pid\":1,\"tid\":{},\"args\":{{\"entity\":\"{}\",\"plugin\":\"{}\",\"calls\":{},\"wait_ns\":{}}}}}", json_escape(&e.id), json_escape(e.phase), json_escape(e.kind), sample.start_ns as f64 / 1e3, sample.stats.total_ns as f64 / 1e3, sample.thread, json_escape(&e.id), json_escape(&e.plugin), sample.stats.calls, sample.stats.wait_ns);
            }
        }
        out.push_str("]}\n");
        std::fs::write(path, out)
    }
}
