// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-18 flight recorder. Bounded history follows local godotwind's
//! performance_profiler.gd and spdlog/details/backtracer-inl.h. System events
//! reuse the PRF-12 clock. Dynamic spans use category names with tracing off.
//! Chrome events follow this crate's existing exporter. Godot's local
//! ProjectSettings docs supply the host's user-data directory contract.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{Operation, System, WorkCounts, WorkRecorder};

const CAPACITY: usize = 65_536;
const HISTORY: Duration = Duration::from_secs(5);
const AFTER: Duration = Duration::from_secs(1);
const COOLDOWN: Duration = Duration::from_secs(10);
const THRESHOLD: Duration = Duration::from_micros(33_400);
static NEXT_DUMP_NS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
enum Kind {
    Span(System),
    NamedSpan {
        name: Cow<'static, str>,
        category: &'static str,
        payload: Option<u64>,
    },
    Counter(Operation, u64),
    Frame,
}

#[derive(Clone)]
struct Event {
    start: Instant,
    duration_ns: u64,
    kind: Kind,
}

impl Event {
    fn end(&self) -> Instant {
        self.start + Duration::from_nanos(self.duration_ns)
    }
}

struct Pending {
    cutoff: Instant,
    trigger: Instant,
    frame_ns: u64,
}

struct Recorder {
    events: VecDeque<Event>,
    threshold: Duration,
    pending: Option<Pending>,
    last_dump: Option<Instant>,
    overwritten: u64,
}

impl Recorder {
    fn new() -> Self {
        Self {
            events: VecDeque::with_capacity(CAPACITY),
            threshold: THRESHOLD,
            pending: None,
            last_dump: None,
            overwritten: 0,
        }
    }

    fn push(&mut self, event: Event, now: Instant) {
        let cutoff = self.pending.as_ref().map_or_else(
            || now.checked_sub(HISTORY).unwrap_or(now),
            |pending| pending.cutoff,
        );
        while self
            .events
            .front()
            .is_some_and(|event| event.end() < cutoff)
        {
            self.events.pop_front();
        }
        if self.events.len() == CAPACITY {
            self.events.pop_front();
            self.overwritten += 1;
        }
        self.events.push_back(event);
    }

    fn frame(&mut self, frame_ns: u64, counts: &WorkCounts, now: Instant) {
        self.push(
            Event {
                start: now
                    .checked_sub(Duration::from_nanos(frame_ns))
                    .unwrap_or(now),
                duration_ns: frame_ns,
                kind: Kind::Frame,
            },
            now,
        );
        for operation in Operation::ALL {
            self.push(
                Event {
                    start: now,
                    duration_ns: 0,
                    kind: Kind::Counter(operation, counts.get(operation)),
                },
                now,
            );
        }
        if self.pending.is_none()
            && Duration::from_nanos(frame_ns) > self.threshold
            && self
                .last_dump
                .is_none_or(|last| now.duration_since(last) >= COOLDOWN)
        {
            self.pending = Some(Pending {
                cutoff: now.checked_sub(HISTORY).unwrap_or(now),
                trigger: now,
                frame_ns,
            });
        }
    }

    fn take_ready(&mut self, now: Instant) -> Option<Capture> {
        let pending = self.pending.as_ref()?;
        let end = pending.trigger + AFTER;
        if now < end {
            return None;
        }
        let events = self
            .events
            .iter()
            .filter(|event| event.end() >= pending.cutoff && event.start <= end)
            .cloned()
            .collect();
        let capture = Capture {
            events,
            cutoff: pending.cutoff,
            end,
            trigger: pending.trigger,
            frame_ns: pending.frame_ns,
            overwritten: self.overwritten,
            thread: crate::thread_id(),
        };
        self.pending = None;
        self.last_dump = Some(now);
        self.overwritten = 0;
        Some(capture)
    }
}

struct Capture {
    events: Vec<Event>,
    cutoff: Instant,
    end: Instant,
    trigger: Instant,
    frame_ns: u64,
    overwritten: u64,
    thread: u64,
}

impl Capture {
    fn write(&self, directory: &Path) -> io::Result<PathBuf> {
        std::fs::create_dir_all(directory)?;
        let stamp = self.trigger.duration_since(crate::epoch()).as_nanos();
        let stem = format!("hitch-{}-{}-{stamp}", std::process::id(), self.thread);
        let path = directory.join(format!("{stem}.json"));
        let mut json = String::from("{\"traceEvents\":[");
        let mut worst: HashMap<&str, u64> = HashMap::new();
        for (index, event) in self.events.iter().enumerate() {
            if index != 0 {
                json.push(',');
            }
            let start = event.start.max(self.cutoff);
            let duration = event.end().min(self.end).saturating_duration_since(start);
            let ts = start.duration_since(self.cutoff).as_secs_f64() * 1e6;
            let (name, category, payload) = match &event.kind {
                Kind::Span(system) => (system.name(), "system", None),
                Kind::NamedSpan {
                    name,
                    category,
                    payload,
                } => (name.as_ref(), *category, *payload),
                Kind::Counter(operation, _) => (operation.name(), "work", None),
                Kind::Frame => ("frame", "frame", None),
            };
            if matches!(event.kind, Kind::Span(_) | Kind::NamedSpan { .. }) {
                let value = worst.entry(name).or_default();
                *value = (*value).max(event.duration_ns);
            }
            let _ = write!(
                json,
                "{{\"name\":\"{}\",\"cat\":\"{}\",\"ts\":{ts:.3},\"pid\":{},\"tid\":{}",
                crate::json_escape(name),
                crate::json_escape(category),
                std::process::id(),
                self.thread
            );
            if let Kind::Counter(_, value) = &event.kind {
                let _ = write!(json, ",\"ph\":\"C\",\"args\":{{\"count\":{value}}}}}");
            } else {
                let _ = write!(
                    json,
                    ",\"ph\":\"X\",\"dur\":{:.3},\"args\":{{",
                    duration.as_secs_f64() * 1e6
                );
                if let Some(payload) = payload {
                    let _ = write!(json, "\"payload\":{payload}");
                }
                json.push_str("}}");
            }
        }
        let _ = writeln!(
            json,
            "],\"hitch\":{{\"trigger_us\":{:.3},\"frame_ms\":{:.3},\"overwritten_events\":{}}}}}",
            self.trigger.duration_since(self.cutoff).as_secs_f64() * 1e6,
            self.frame_ns as f64 / 1e6,
            self.overwritten,
        );
        let mut slowest: Vec<_> = worst.into_iter().collect();
        slowest.sort_by(|(a, ta), (b, tb)| tb.cmp(ta).then_with(|| a.cmp(b)));
        let names: Vec<_> = slowest
            .iter()
            .filter(|(_, duration)| *duration != 0)
            .take(3)
            .map(|(name, duration)| {
                format!(
                    "{}={:.3}ms",
                    name.replace(['\n', '\r'], " "),
                    *duration as f64 / 1e6
                )
            })
            .collect();
        let summary = format!(
            "SOE hitch {:.3}ms slowest [{}] overwritten={} trace={}\n",
            self.frame_ns as f64 / 1e6,
            names.join(", "),
            self.overwritten,
            path.display().to_string().replace(['\n', '\r'], " ")
        );
        std::fs::write(&path, json)?;
        std::fs::write(directory.join(format!("{stem}.txt")), &summary)?;
        eprint!("{summary}");
        Ok(path)
    }
}

struct FlightRecorder {
    recorder: Recorder,
    directory: PathBuf,
    writer: Option<JoinHandle<io::Result<()>>>,
}

impl Drop for FlightRecorder {
    fn drop(&mut self) {
        if let Err(error) = join_writer(&mut self.writer) {
            eprintln!("SOE hitch export: {error}");
        }
    }
}

fn default_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("SOE_HITCH_DIR") {
        return PathBuf::from(path);
    }
    let base = std::env::var_os("XDG_DATA_HOME").map_or_else(
        || {
            std::env::var_os("HOME").map_or_else(std::env::temp_dir, |home| {
                PathBuf::from(home).join(".local/share")
            })
        },
        PathBuf::from,
    );
    base.join("godot/app_userdata/Skyrim open engine host/hitches")
}

thread_local! {
    static FLIGHT: RefCell<FlightRecorder> = RefCell::new(FlightRecorder {
        recorder: Recorder::new(),
        directory: default_directory(),
        writer: None,
    });
}

pub(crate) fn prepare() {
    let _ = crate::epoch();
    FLIGHT.with(|_| {});
}

pub(crate) fn span(system: System, start: Instant, duration_ns: u64, now: Instant) {
    FLIGHT.with(|flight| {
        flight.borrow_mut().recorder.push(
            Event {
                start,
                duration_ns,
                kind: Kind::Span(system),
            },
            now,
        );
    });
}

pub(crate) fn named_span(event: crate::Event, start: Instant, now: Instant) {
    FLIGHT.with(|flight| {
        flight.borrow_mut().recorder.push(
            Event {
                start,
                duration_ns: event.duration_ns,
                kind: Kind::NamedSpan {
                    name: event.name,
                    category: event.category,
                    payload: event.payload,
                },
            },
            now,
        );
    });
}

pub(crate) fn frame(frame_ns: u64) {
    let now = Instant::now();
    let counts = WorkRecorder::current().map_or_else(WorkCounts::default, |work| work.snapshot());
    FLIGHT.with(|flight| {
        let mut flight = flight.borrow_mut();
        flight.recorder.frame(frame_ns, &counts, now);
        if flight
            .writer
            .as_ref()
            .is_some_and(|writer| !writer.is_finished())
        {
            return;
        }
        if let Some(capture) = flight.recorder.take_ready(now) {
            let now_ns =
                u64::try_from(now.duration_since(crate::epoch()).as_nanos()).unwrap_or(u64::MAX);
            if !claim_dump(&NEXT_DUMP_NS, now_ns) {
                return;
            }
            if let Err(error) = join_writer(&mut flight.writer) {
                eprintln!("SOE hitch export: {error}");
            }
            let directory = flight.directory.clone();
            match std::thread::Builder::new()
                .name("soe-hitch-export".into())
                .spawn(move || capture.write(&directory).map(|_| ()))
            {
                Ok(writer) => flight.writer = Some(writer),
                Err(error) => eprintln!("SOE hitch writer: {error}"),
            }
        }
    });
}

fn claim_dump(next_dump: &AtomicU64, now_ns: u64) -> bool {
    let next = next_dump.load(Ordering::Relaxed);
    now_ns >= next
        && next_dump
            .compare_exchange(
                next,
                now_ns.saturating_add(10_000_000_000),
                Ordering::Relaxed,
                Ordering::Relaxed,
            )
            .is_ok()
}

pub(crate) fn set_directory(path: &Path) {
    FLIGHT.with(|flight| path.clone_into(&mut flight.borrow_mut().directory));
}

pub(crate) fn set_threshold(threshold: Duration) -> io::Result<()> {
    if threshold.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hitch threshold must be positive",
        ));
    }
    FLIGHT.with(|flight| flight.borrow_mut().recorder.threshold = threshold);
    Ok(())
}

fn join_writer(writer: &mut Option<JoinHandle<io::Result<()>>>) -> io::Result<()> {
    if let Some(writer) = writer.take() {
        writer
            .join()
            .map_err(|_| io::Error::other("hitch writer panicked"))??;
    }
    Ok(())
}

pub(crate) fn wait_for_exports() -> io::Result<()> {
    FLIGHT.with(|flight| join_writer(&mut flight.borrow_mut().writer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(now: Instant, system: System) -> Event {
        Event {
            start: now,
            duration_ns: 1,
            kind: Kind::Span(system),
        }
    }

    #[test]
    fn prf_18_dump_limit_is_shared_across_recording_threads() {
        let next = AtomicU64::new(0);
        std::thread::scope(|threads| {
            let handles: Vec<_> = (0..4)
                .map(|_| threads.spawn(|| claim_dump(&next, 1)))
                .collect();
            assert_eq!(
                handles
                    .into_iter()
                    .map(|handle| usize::from(handle.join().unwrap()))
                    .sum::<usize>(),
                1
            );
        });
        assert!(!claim_dump(&next, 10_000_000_000));
        assert!(claim_dump(&next, 10_000_000_001));
    }

    #[test]
    fn prf_18_ring_retains_five_seconds_and_evicts_older_events() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        recorder.push(event(start, System::Input), start);
        recorder.push(event(start + HISTORY, System::Combat), start + HISTORY);
        assert_eq!(recorder.events.len(), 2);
        recorder.push(
            event(start + HISTORY + AFTER, System::Magic),
            start + HISTORY + AFTER,
        );
        assert_eq!(recorder.events.len(), 2);
        assert!(matches!(
            recorder.events[0].kind,
            Kind::Span(System::Combat)
        ));
    }

    #[test]
    fn prf_18_ring_has_fixed_capacity_and_reports_overwrites() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        for _ in 0..CAPACITY + 7 {
            recorder.push(event(start, System::Combat), start);
        }
        assert_eq!(recorder.events.len(), CAPACITY);
        assert_eq!(recorder.events.capacity(), CAPACITY);
        assert_eq!(recorder.overwritten, 7);
    }

    #[test]
    fn prf_18_threshold_is_strictly_twice_the_frame_budget_and_is_configurable() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        assert_eq!(recorder.threshold, Duration::from_micros(16_700) * 2);
        recorder.frame(33_400_000, &WorkCounts::default(), start);
        assert!(recorder.pending.is_none());
        recorder.frame(33_400_001, &WorkCounts::default(), start);
        assert!(recorder.pending.is_some());
        assert_eq!(
            set_threshold(Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        set_threshold(Duration::from_millis(100)).unwrap();
        FLIGHT.with(|flight| {
            assert_eq!(
                flight.borrow().recorder.threshold,
                Duration::from_millis(100)
            );
        });
    }

    #[test]
    fn prf_18_capture_pins_preceding_frames_and_waits_one_second_after_hitch() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        for second in 0..=5 {
            let now = start + Duration::from_secs(second);
            recorder.frame(1_000_000, &WorkCounts::default(), now);
        }
        let hitch = start + HISTORY;
        recorder.frame(50_000_000, &WorkCounts([7; 10]), hitch);
        assert!(recorder
            .take_ready(hitch + Duration::from_nanos(999_999_999))
            .is_none());
        recorder.frame(1_000_000, &WorkCounts([9; 10]), hitch + AFTER);
        let capture = recorder.take_ready(hitch + AFTER).unwrap();
        assert!(capture.events.iter().any(|event| event.start <= start));
        assert!(capture
            .events
            .iter()
            .any(|event| matches!(event.kind, Kind::Counter(_, 9))));
        assert_eq!(capture.end, hitch + AFTER);
    }

    #[test]
    fn prf_18_repeated_hitches_do_not_extend_tail_and_dumps_have_ten_second_cooldown() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        recorder.frame(50_000_000, &WorkCounts::default(), start);
        recorder.frame(
            90_000_000,
            &WorkCounts::default(),
            start + Duration::from_millis(900),
        );
        assert!(recorder.take_ready(start + AFTER).is_some());
        recorder.frame(
            50_000_000,
            &WorkCounts::default(),
            start + Duration::from_nanos(10_999_999_999),
        );
        assert!(recorder.pending.is_none());
        recorder.frame(50_000_000, &WorkCounts::default(), start + AFTER + COOLDOWN);
        assert!(recorder.pending.is_some());
    }

    #[test]
    fn prf_18_export_contains_chrome_spans_counters_and_one_line_slowest_summary() {
        let mut recorder = Recorder::new();
        let start = Instant::now();
        recorder.push(
            Event {
                start,
                duration_ns: 50_000_000,
                kind: Kind::Span(System::Papyrus),
            },
            start + Duration::from_millis(50),
        );
        recorder.push(
            Event {
                start,
                duration_ns: 60_000_000,
                kind: Kind::NamedSpan {
                    name: Cow::Borrowed("static\"slow\nspan"),
                    category: "test",
                    payload: Some(42),
                },
            },
            start + Duration::from_millis(60),
        );
        recorder.frame(
            50_000_000,
            &WorkCounts([7; 10]),
            start + Duration::from_millis(50),
        );
        let capture = recorder
            .take_ready(start + AFTER + Duration::from_millis(50))
            .unwrap();
        let directory = std::env::temp_dir().join(format!("prf-18-export-{}", std::process::id()));
        let path = capture.write(&directory).unwrap();
        let json = std::fs::read_to_string(&path).unwrap();
        assert!(json.contains("\"ph\":\"C\",\"args\":{\"count\":7}"));
        assert!(json.contains("\"ph\":\"X\",\"dur\":50000.000"));
        assert!(json.contains("\"name\":\"papyrus_vm\""));
        assert!(json.contains("\"name\":\"static\\\"slow\\u000aspan\""));
        assert!(json.contains("\"payload\":42"));
        let summary = std::fs::read_to_string(path.with_extension("txt")).unwrap();
        assert_eq!(summary.lines().count(), 1);
        assert!(summary.contains("papyrus_vm=50.000ms"));
        let error = capture.write(&path).unwrap_err();
        assert!(matches!(
            error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::NotADirectory
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
