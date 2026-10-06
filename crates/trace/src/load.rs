// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded load diagnostics using the repository's Instant scopes. Cause
//! boundaries follow docs/performance.md. Godot's pipeline compilation guide
//! supplies counts, not compiler timings, so those cannot be inferred here.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The kind of work measured at a load boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadCause {
    /// Elapsed file operations, including archive read lock contention.
    IoWait,
    /// Turning container bytes into records or asset structures.
    Parse,
    /// Decompression or pixel/animation decoding.
    Decode,
    /// Constructing CPU-side meshes, collision, navigation or form tables.
    CpuBuild,
    /// Host upload submission, including waits inside the server call.
    GpuUpload,
    /// Shader compilation when directly observable.
    ShaderCompile,
    /// Pipeline compilation when directly observable.
    PipelineCompile,
    /// Time between submitting work and a worker starting it.
    WorkerQueue,
}

impl LoadCause {
    /// Stable diagnostic name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::IoWait => "io_wait",
            Self::Parse => "parse",
            Self::Decode => "decode",
            Self::CpuBuild => "cpu_build",
            Self::GpuUpload => "gpu_upload",
            Self::ShaderCompile => "shader_compile",
            Self::PipelineCompile => "pipeline_compile",
            Self::WorkerQueue => "worker_queue",
        }
    }
}

/// A completed load phase. Worker durations are not main-thread frame shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadEvent {
    /// Unique scope identity.
    pub id: u64,
    /// Outermost operation identity, retained across workers.
    pub root: u64,
    /// Parent scope, if any.
    pub parent: Option<u64>,
    /// Named implementation boundary.
    pub phase: &'static str,
    /// None for an aggregate operation or object.
    pub cause: Option<LoadCause>,
    /// File path or record identity.
    pub asset: String,
    /// Owning base form or object, inherited across child phases and workers.
    pub object: String,
    /// Nanoseconds from the trace epoch.
    pub start_ns: u64,
    /// Inclusive elapsed wall time on this thread.
    pub duration_ns: u64,
    /// Elapsed time excluding nested scopes on this thread.
    pub exclusive_ns: u64,
    /// Budget, set only on aggregate operations.
    pub budget_ns: Option<u64>,
    /// Trace thread identity.
    pub thread: u64,
    /// A sum of disjoint calls, exported as a counter rather than a time interval.
    pub accumulated: bool,
}

const MAX_LOAD_EVENTS: usize = 65_536;
static NEXT_LOAD: AtomicU64 = AtomicU64::new(1);
static EVENTS: Mutex<VecDeque<LoadEvent>> = Mutex::new(VecDeque::new());
static DROPPED: AtomicU64 = AtomicU64::new(0);

struct Context {
    id: u64,
    root: u64,
    object: String,
    children_ns: AtomicU64,
}

thread_local! {
    static ACTIVE: RefCell<Option<(Arc<Context>, bool)>> = const { RefCell::new(None) };
}

/// Captures the current load's ownership for worker tasks.
#[derive(Clone)]
pub struct LoadToken(Arc<Context>);

impl LoadToken {
    /// Current load context, if this thread is loading.
    #[must_use]
    pub fn current() -> Option<Self> {
        ACTIVE.with(|active| active.borrow().as_ref().map(|(ctx, _)| Self(ctx.clone())))
    }

    /// Restore ownership on a worker without charging worker time to the
    /// submitting thread. Drop the guard before leaving the worker task.
    pub fn enter(&self) -> LoadWorker {
        let previous = ACTIVE.with(|active| active.replace(Some((self.0.clone(), false))));
        LoadWorker {
            previous,
            thread: PhantomData,
        }
    }
}

/// Thread-bound worker ownership guard.
pub struct LoadWorker {
    previous: Option<(Arc<Context>, bool)>,
    thread: PhantomData<Rc<()>>,
}

impl Drop for LoadWorker {
    fn drop(&mut self) {
        ACTIVE.with(|active| active.replace(self.previous.take()));
    }
}

/// Always-on load scope. No profiling state is written to game saves.
#[must_use]
pub struct LoadScope {
    context: Arc<Context>,
    previous: Option<(Arc<Context>, bool)>,
    event: LoadEvent,
    start: Instant,
    thread: PhantomData<Rc<()>>,
}

impl LoadScope {
    /// Measure a concrete cause phase and its asset, including failed loads.
    pub fn new(phase: &'static str, cause: LoadCause, asset: impl Into<String>) -> Self {
        Self::begin(phase, Some(cause), asset.into(), None, None)
    }

    /// Measure an aggregate load with an explicit budget.
    pub fn operation(phase: &'static str, asset: impl Into<String>, budget: Duration) -> Self {
        Self::begin(phase, None, asset.into(), Some(nanos(budget)), None)
    }

    /// Attribute nested assets to an owning base form or object.
    pub fn object(object: impl Into<String>) -> Self {
        let object = object.into();
        Self::begin("object", None, object.clone(), None, Some(object))
    }

    fn begin(
        phase: &'static str,
        cause: Option<LoadCause>,
        asset: String,
        budget_ns: Option<u64>,
        object: Option<String>,
    ) -> Self {
        let id = NEXT_LOAD.fetch_add(1, Ordering::Relaxed);
        let previous = ACTIVE.with(|active| active.borrow().clone());
        let root = previous.as_ref().map_or(id, |(ctx, _)| ctx.root);
        let object = object.unwrap_or_else(|| {
            previous
                .as_ref()
                .map_or_else(String::new, |(ctx, _)| ctx.object.clone())
        });
        let context = Arc::new(Context {
            id,
            root,
            object: object.clone(),
            children_ns: AtomicU64::new(0),
        });
        ACTIVE.with(|active| active.replace(Some((context.clone(), true))));
        let _ = crate::epoch();
        let start = Instant::now();
        Self {
            context,
            previous: previous.clone(),
            start,
            thread: PhantomData,
            event: LoadEvent {
                id,
                root,
                parent: previous.map(|(ctx, _)| ctx.id),
                phase,
                cause,
                asset,
                object,
                start_ns: nanos(start.duration_since(crate::epoch())),
                duration_ns: 0,
                exclusive_ns: 0,
                budget_ns,
                thread: crate::thread_id(),
                accumulated: false,
            },
        }
    }

    /// Identity for querying this operation after it completes.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.context.id
    }
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

impl Drop for LoadScope {
    fn drop(&mut self) {
        self.event.duration_ns = nanos(self.start.elapsed());
        self.event.exclusive_ns = self
            .event
            .duration_ns
            .saturating_sub(self.context.children_ns.load(Ordering::Relaxed));
        if let Some((parent, true)) = &self.previous {
            parent
                .children_ns
                .fetch_add(self.event.duration_ns, Ordering::Relaxed);
        }
        ACTIVE.with(|active| active.replace(self.previous.take()));
        let event = self.event.clone();
        if let Ok(mut events) = EVENTS.lock() {
            if events.len() == MAX_LOAD_EVENTS {
                events.pop_front();
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
            events.push_back(event.clone());
        }
        if event.budget_ns.is_some() && (event.parent.is_none() || over_budget(&event)) {
            crate::info!(target: "load", system = "load", "{}", LoadTrace::operation_report(&event));
        }
    }
}

fn over_budget(event: &LoadEvent) -> bool {
    event
        .budget_ns
        .is_some_and(|budget| event.duration_ns > budget)
}

/// Query and export the bounded load history without enabling Chrome tracing.
pub struct LoadTrace;

impl LoadTrace {
    /// Record a submitted chunk's elapsed queue wait at worker entry. This
    /// interval overlaps the submitter's wall time and is not subtracted from it.
    pub fn queue_wait(asset: &str, submitted: Instant) {
        let Some(token) = LoadToken::current() else {
            return;
        };
        let duration_ns = nanos(submitted.elapsed());
        let event = LoadEvent {
            id: NEXT_LOAD.fetch_add(1, Ordering::Relaxed),
            root: token.0.root,
            parent: Some(token.0.id),
            phase: "worker_queue",
            cause: Some(LoadCause::WorkerQueue),
            asset: asset.to_string(),
            object: token.0.object.clone(),
            start_ns: nanos(submitted.duration_since(crate::epoch())),
            duration_ns,
            exclusive_ns: duration_ns,
            budget_ns: None,
            thread: crate::thread_id(),
            accumulated: false,
        };
        if let Ok(mut events) = EVENTS.lock() {
            if events.len() == MAX_LOAD_EVENTS {
                events.pop_front();
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
            events.push_back(event);
        }
    }

    /// Record the sum of short calls within the active scope. Charge their
    /// elapsed time to the parent once and export a counter at completion.
    pub fn accumulated(phase: &'static str, cause: LoadCause, asset: &str, elapsed: Duration) {
        let Some(token) = LoadToken::current() else {
            return;
        };
        let duration_ns = nanos(elapsed);
        if ACTIVE.with(|active| active.borrow().as_ref().is_some_and(|(_, local)| *local)) {
            token
                .0
                .children_ns
                .fetch_add(duration_ns, Ordering::Relaxed);
        }
        let event = LoadEvent {
            id: NEXT_LOAD.fetch_add(1, Ordering::Relaxed),
            root: token.0.root,
            parent: Some(token.0.id),
            phase,
            cause: Some(cause),
            asset: asset.to_owned(),
            object: token.0.object.clone(),
            start_ns: nanos(crate::epoch().elapsed()),
            duration_ns,
            exclusive_ns: duration_ns,
            budget_ns: None,
            thread: crate::thread_id(),
            accumulated: true,
        };
        if let Ok(mut events) = EVENTS.lock() {
            if events.len() == MAX_LOAD_EVENTS {
                events.pop_front();
                DROPPED.fetch_add(1, Ordering::Relaxed);
            }
            events.push_back(event);
        }
    }

    /// Retained events in start order. The summary reports evicted events.
    #[must_use]
    pub fn events() -> Vec<LoadEvent> {
        let mut events = EVENTS
            .lock()
            .map(|events| events.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        events.sort_by_key(|event| (event.start_ns, event.id));
        events
    }

    /// Largest exclusive cause phase in the operation's subtree.
    #[must_use]
    pub fn slowest(operation: &LoadEvent) -> Option<LoadEvent> {
        let events = EVENTS.lock().ok()?;
        if operation.id == operation.root {
            return events
                .iter()
                .filter(|event| event.root == operation.id && event.cause.is_some())
                .max_by_key(|event| (event.exclusive_ns, std::cmp::Reverse(event.id)))
                .cloned();
        }
        let mut relevant: Vec<_> = events
            .iter()
            .filter(|event| event.root == operation.root)
            .collect();
        relevant.sort_unstable_by_key(|event| event.id);
        let mut descendants = std::collections::HashSet::from([operation.id]);
        let mut slowest: Option<&LoadEvent> = None;
        for event in relevant {
            if event
                .parent
                .is_some_and(|parent| descendants.contains(&parent))
            {
                descendants.insert(event.id);
            }
            if event.cause.is_some()
                && descendants.contains(&event.id)
                && slowest.is_none_or(|previous| {
                    (event.exclusive_ns, std::cmp::Reverse(event.id))
                        > (previous.exclusive_ns, std::cmp::Reverse(previous.id))
                })
            {
                slowest = Some(event);
            }
        }
        slowest.cloned()
    }

    /// Budget status, slowest asset and its cause, shared by log and dump.
    #[must_use]
    pub fn operation_report(event: &LoadEvent) -> String {
        let slowest = Self::slowest(event);
        format!("LOAD phase={} asset={:?} ms={:.3} budget_ms={:.3} over_budget={} slowest={:?} object={:?} cause={} exclusive_ms={:.3}",
            event.phase, event.asset, event.duration_ns as f64 / 1e6, event.budget_ns.unwrap_or(0) as f64 / 1e6,
            over_budget(event), slowest.as_ref().map_or("unattributed", |s| s.asset.as_str()),
            slowest.as_ref().map_or("", |s| s.object.as_str()),
            slowest.as_ref().and_then(|s| s.cause).map_or("unattributed", LoadCause::name),
            slowest.as_ref().map_or(0, |s| s.exclusive_ns) as f64 / 1e6)
    }

    /// Full phase table followed by aggregate budget reports.
    #[must_use]
    pub fn summary_text() -> String {
        let events = Self::events();
        let mut out = format!(
            "load_events={} evicted={}\n",
            events.len(),
            DROPPED.load(Ordering::Relaxed)
        );
        for event in &events {
            let _ = writeln!(out, "LOAD_PHASE id={} parent={:?} phase={} cause={} asset={:?} object={:?} ms={:.3} exclusive_ms={:.3} thread={} accumulated={}",
                event.id, event.parent, event.phase, event.cause.map_or("aggregate", LoadCause::name),
                event.asset, event.object, event.duration_ns as f64 / 1e6, event.exclusive_ns as f64 / 1e6, event.thread, event.accumulated);
        }
        for event in events.iter().filter(|event| event.budget_ns.is_some()) {
            let _ = writeln!(out, "{}", Self::operation_report(event));
        }
        out
    }
}

pub(crate) fn append_chrome_events(out: &mut String, mut comma: bool) {
    for event in LoadTrace::events() {
        if comma {
            out.push(',');
        }
        comma = true;
        if event.accumulated {
            let _ = write!(out, "{{\"name\":\"{}\",\"cat\":\"load.{}\",\"ph\":\"C\",\"ts\":{:.3},\"pid\":1,\"tid\":{},\"args\":{{\"elapsed_ns\":{}}}}}",
                crate::json_escape(&format!("{} {}", event.phase, event.asset)), event.cause.map_or("aggregate", LoadCause::name),
                event.start_ns as f64 / 1e3, event.thread, event.duration_ns);
            continue;
        }
        let _ = write!(out, "{{\"name\":\"{}\",\"cat\":\"load.{}\",\"ph\":\"X\",\"ts\":{:.3},\"dur\":{:.3},\"pid\":1,\"tid\":{},\"args\":{{\"id\":{},\"root\":{},\"asset\":\"{}\",\"object\":\"{}\",\"exclusive_ns\":{}}}}}",
            crate::json_escape(event.phase), event.cause.map_or("aggregate", LoadCause::name),
            event.start_ns as f64 / 1e3, event.duration_ns as f64 / 1e3, event.thread, event.id, event.root,
            crate::json_escape(&event.asset), crate::json_escape(&event.object), event.exclusive_ns);
    }
}
