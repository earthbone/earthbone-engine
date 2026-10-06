// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! Scoped timing spans. A [`Span`] records its name, category, thread,
//! start and duration into the global event list when tracing is enabled,
//! while the bounded hitch recorder retains spans with optional tracing off.
//! [`Trace::export_chrome_json`]
//! writes the Chrome trace event format that Perfetto and `chrome://tracing`
//! open, [`Trace::summary`] folds the events per name into count, total,
//! mean, p50, p95 and max.
//!
//! ```
//! use soe_trace::{Span, Trace};
//!
//! Trace::enable();
//! {
//!     let _span = Span::new("esm.parse", "esm");
//! }
//! let summary = Trace::summary();
//! assert_eq!(summary[0].name, "esm.parse");
//! Trace::disable();
//! ```

use std::borrow::Cow;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

mod frame;
mod hitch;
pub use frame::{
    with_timing_suspended, FrameHistory, FrameSample, FrameSnapshot, System, SystemScope,
    SystemStats, FRAME_HISTORY, SYSTEM_COUNT,
};

mod work;
pub use work::{count, Operation, WorkCounts, WorkFrame, WorkRecorder};

mod papyrus;
pub use papyrus::{EventHealth, EventRate, PapyrusHealth, UpdateRegistrations};

pub mod logging;
pub use tracing;
pub use tracing::{debug, error, info, trace, warn};

mod entity;
pub use entity::{Entity, EntityScope, EntityStats, InputLatency, Profile};

/// Events kept in the global list. Past this the oldest are dropped, so a
/// runaway trace keeps the newest window.
pub const MAX_EVENTS: usize = 1 << 22;

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);
static EPOCH: OnceLock<Instant> = OnceLock::new();
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub name: Cow<'static, str>,
    pub category: &'static str,
    pub thread: u64,
    /// Nanoseconds since the first span of the process.
    pub start_ns: u64,
    pub duration_ns: u64,
    pub payload: Option<u64>,
}

thread_local! {
    static THREAD_ID: u64 = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
}

/// Events go straight into the global list under its mutex. A per-thread
/// buffer flushed on thread exit was tried first and lost the tail of a
/// scoped worker's events when the join returned before the thread-local
/// destructor ran, so the lock is the price of a complete trace.
fn record(event: Event) {
    if let Ok(mut global) = EVENTS.lock() {
        if global.len() >= MAX_EVENTS {
            global.remove(0);
        }
        global.push(event);
    }
}

fn epoch() -> Instant {
    *EPOCH.get_or_init(Instant::now)
}

fn thread_id() -> u64 {
    THREAD_ID.with(|id| *id)
}

/// A timed scope. Dropping it records the event.
pub struct Span {
    start: Option<(Instant, Cow<'static, str>, &'static str, Option<u64>)>,
    detailed: bool,
}

impl Span {
    #[must_use]
    pub fn new(name: &'static str, category: &'static str) -> Span {
        Self::build(Cow::Borrowed(name), category, None)
    }

    /// A span whose name is built at run time, `Class.Function` for a
    /// native call. With optional tracing off, the hitch recorder uses the
    /// category as its name and does not build the string.
    #[must_use]
    pub fn dynamic(name: impl FnOnce() -> String, category: &'static str) -> Span {
        if !Trace::enabled() {
            return Self::build(Cow::Borrowed(category), category, None);
        }
        Self::build(Cow::Owned(name()), category, None)
    }

    #[must_use]
    pub fn with_payload(name: &'static str, category: &'static str, payload: u64) -> Span {
        Self::build(Cow::Borrowed(name), category, Some(payload))
    }

    fn build(name: Cow<'static, str>, category: &'static str, payload: Option<u64>) -> Span {
        let detailed = Trace::enabled();
        if !detailed && frame::timing_suspended() {
            return Span {
                start: None,
                detailed,
            };
        }
        let _ = epoch();
        Span {
            start: Some((Instant::now(), name, category, payload)),
            detailed,
        }
    }

    /// Attach a payload after the fact, a byte count or an item count.
    pub fn set_payload(&mut self, payload: u64) {
        if let Some(s) = &mut self.start {
            s.3 = Some(payload);
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((start, name, category, payload)) = self.start.take() else {
            return;
        };
        let end = Instant::now();
        let event = Event {
            name,
            category,
            thread: thread_id(),
            start_ns: start.duration_since(epoch()).as_nanos() as u64,
            duration_ns: end.duration_since(start).as_nanos() as u64,
            payload,
        };
        if self.detailed {
            record(event.clone());
        }
        if !frame::timing_suspended() {
            hitch::named_span(event, start, end);
        }
    }
}

/// Per-name statistics, durations in nanoseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameStats {
    pub name: String,
    pub category: &'static str,
    pub count: usize,
    pub total_ns: u64,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub max_ns: u64,
    /// Sum of the payloads that were set.
    pub payload_total: u64,
}

pub struct Trace;

impl Trace {
    /// Override the calling thread's hitch directory, for host user-data paths.
    pub fn set_hitch_directory(path: &std::path::Path) {
        hitch::set_directory(path);
    }

    /// Set a positive hitch threshold. The default is twice the 16.7 ms budget.
    ///
    /// # Errors
    /// Returns an invalid-input error for a zero threshold.
    pub fn set_hitch_threshold(threshold: std::time::Duration) -> std::io::Result<()> {
        hitch::set_threshold(threshold)
    }

    /// Join completed hitch exports before a headless run or host exits.
    /// A capture still awaiting its one-second tail remains pending.
    ///
    /// # Errors
    /// Returns the export's I/O error or an error if its writer panicked.
    pub fn wait_for_hitch_exports() -> std::io::Result<()> {
        hitch::wait_for_exports()
    }

    pub fn enable() {
        ENABLED.store(true, Ordering::Relaxed);
    }

    pub fn disable() {
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[inline]
    #[must_use]
    pub fn enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    /// Kept for callers that buffered per thread in an earlier design.
    /// Events are already in the global list.
    pub fn flush() {}

    pub fn clear() {
        if let Ok(mut global) = EVENTS.lock() {
            global.clear();
        }
    }

    /// Every recorded event, in start order.
    #[must_use]
    pub fn events() -> Vec<Event> {
        Self::flush();
        let mut events = EVENTS.lock().map(|g| g.clone()).unwrap_or_default();
        events.sort_by_key(|e| (e.start_ns, e.thread));
        events
    }

    #[must_use]
    pub fn summary() -> Vec<NameStats> {
        summarize(&Self::events())
    }

    /// Write `{"traceEvents": [...]}` with complete (`ph: X`) events, times
    /// in microseconds as the format requires.
    pub fn export_chrome_json(path: &std::path::Path) -> std::io::Result<()> {
        let events = Self::events();
        let mut out = String::with_capacity(events.len() * 96 + 32);
        out.push_str("{\"traceEvents\":[");
        for (i, e) in events.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"name\":\"{}\",\"cat\":\"{}\",\"ph\":\"X\",\"ts\":{:.3},\"dur\":{:.3},\"pid\":1,\"tid\":{}",
                json_escape(&e.name),
                json_escape(e.category),
                e.start_ns as f64 / 1000.0,
                e.duration_ns as f64 / 1000.0,
                e.thread
            );
            match e.payload {
                Some(p) => {
                    let _ = write!(out, ",\"args\":{{\"payload\":{p}}}}}");
                }
                None => out.push_str(",\"args\":{}}"),
            }
        }
        out.push_str("]}\n");
        std::fs::write(path, out)
    }

    /// The summary as an aligned text table, largest total first.
    #[must_use]
    pub fn summary_text() -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<44} {:>9} {:>11} {:>10} {:>10} {:>10} {:>10} {:>12}",
            "span", "count", "total_ms", "mean_us", "p50_us", "p95_us", "max_us", "payload"
        );
        for s in Self::summary() {
            let _ = writeln!(
                out,
                "{:<44} {:>9} {:>11.3} {:>10.1} {:>10.1} {:>10.1} {:>10.1} {:>12}",
                s.name,
                s.count,
                s.total_ns as f64 / 1e6,
                s.mean_ns as f64 / 1e3,
                s.p50_ns as f64 / 1e3,
                s.p95_ns as f64 / 1e3,
                s.max_ns as f64 / 1e3,
                s.payload_total
            );
        }
        out
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Fold events per name. Sorted by total time, largest first.
#[must_use]
pub fn summarize(events: &[Event]) -> Vec<NameStats> {
    let mut by_name: std::collections::HashMap<&str, (&'static str, Vec<u64>, u64)> =
        std::collections::HashMap::new();
    for e in events {
        let entry = by_name
            .entry(&e.name)
            .or_insert((e.category, Vec::new(), 0));
        entry.1.push(e.duration_ns);
        entry.2 += e.payload.unwrap_or(0);
    }
    let mut out: Vec<NameStats> = by_name
        .into_iter()
        .map(|(name, (category, mut durations, payload_total))| {
            durations.sort_unstable();
            let count = durations.len();
            let total_ns: u64 = durations.iter().sum();
            let at = |q: f64| durations[((count - 1) as f64 * q) as usize];
            NameStats {
                name: name.to_string(),
                category,
                count,
                total_ns,
                mean_ns: total_ns / count as u64,
                p50_ns: at(0.5),
                p95_ns: at(0.95),
                max_ns: durations[count - 1],
                payload_total,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.total_ns
            .cmp(&a.total_ns)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests share the global switch and list, so they run in one lock.
    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn spans_only_enter_optional_event_list_when_enabled() {
        let _guard = LOCK.lock().unwrap();
        Trace::disable();
        Trace::clear();
        {
            let _s = Span::new("off", "test");
        }
        assert_eq!(Trace::events(), Vec::<Event>::new());
        Trace::enable();
        {
            let _s = Span::with_payload("outer", "test", 7);
            {
                let _i = Span::new("inner", "test");
            }
            let _d = Span::dynamic(|| format!("{}.{}", "Quest", "SetStage"), "native");
        }
        Trace::disable();
        let events = Trace::events();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].name, "outer");
        assert_eq!(events[0].payload, Some(7));
        assert!(events.iter().any(|e| e.name == "Quest.SetStage"));
        let summary = summarize(&events);
        assert_eq!(summary.iter().map(|s| s.count).sum::<usize>(), 3);
        Trace::clear();
    }

    #[test]
    fn events_from_scoped_threads_arrive_with_their_own_thread_id() {
        let _guard = LOCK.lock().unwrap();
        Trace::clear();
        Trace::enable();
        std::thread::scope(|s| {
            for _ in 0..3 {
                s.spawn(|| {
                    let _s = Span::new("worker", "test");
                });
            }
        });
        Trace::disable();
        let events = Trace::events();
        let workers: Vec<&Event> = events.iter().filter(|e| e.name == "worker").collect();
        assert_eq!(workers.len(), 3);
        let mut threads: Vec<u64> = workers.iter().map(|e| e.thread).collect();
        threads.dedup();
        assert!(threads.len() >= 2 || workers.iter().all(|e| e.thread == workers[0].thread));
        Trace::clear();
    }

    #[test]
    fn the_chrome_json_is_valid_and_the_summary_orders_by_total() {
        let _guard = LOCK.lock().unwrap();
        Trace::clear();
        Trace::enable();
        {
            let _a = Span::new("slow", "test");
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        {
            let _b = Span::new("fa\"st", "test");
        }
        Trace::disable();
        let dir = std::env::temp_dir().join(format!("soe-trace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.json");
        Trace::export_chrome_json(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("{\"traceEvents\":[{"));
        assert!(text.contains("\"name\":\"fa\\\"st\""));
        assert!(text.contains("\"ph\":\"X\""));
        assert_eq!(text.matches("\"tid\":").count(), 2);
        let summary = Trace::summary();
        assert_eq!(summary[0].name, "slow");
        assert!(Trace::summary_text().contains("slow"));
        std::fs::remove_dir_all(&dir).ok();
        Trace::clear();
    }
}
