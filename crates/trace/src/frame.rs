// SPDX-License-Identifier: GPL-3.0-or-later

//! Exclusive frame timings over fixed system slots. Bounded samples and
//! query-time percentiles follow godotwind's performance_profiler.gd. The
//! engine's main-thread scheduler needs neither its dynamic names nor a lock.

use std::cell::Cell;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Number of system slots, including work outside the named systems.
pub const SYSTEM_COUNT: usize = 14;
/// Number of completed frames retained for rolling percentiles.
pub const FRAME_HISTORY: usize = 240;

/// Stable slots shared by the headless runtime, host monitors and overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum System {
    Papyrus,
    AiPlanning,
    AiMovement,
    Combat,
    Magic,
    Quests,
    Dialogue,
    Physics,
    Streaming,
    Audio,
    Save,
    Input,
    Render,
    Other,
}

impl System {
    /// Every slot, in report order. Idle systems remain present with zero time.
    pub const ALL: [Self; SYSTEM_COUNT] = [
        Self::Papyrus,
        Self::AiPlanning,
        Self::AiMovement,
        Self::Combat,
        Self::Magic,
        Self::Quests,
        Self::Dialogue,
        Self::Physics,
        Self::Streaming,
        Self::Audio,
        Self::Save,
        Self::Input,
        Self::Render,
        Self::Other,
    ];

    /// Stable identifier used in monitors and summaries.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Papyrus => "papyrus_vm",
            Self::AiPlanning => "ai_planning",
            Self::AiMovement => "ai_movement",
            Self::Combat => "combat",
            Self::Magic => "magic",
            Self::Quests => "quests_story_manager",
            Self::Dialogue => "dialogue",
            Self::Physics => "physics_sync",
            Self::Streaming => "cell_streaming",
            Self::Audio => "audio",
            Self::Save => "save",
            Self::Input => "input",
            Self::Render => "host_render_submission",
            Self::Other => "other",
        }
    }
}

struct Clock {
    totals: [Cell<u64>; SYSTEM_COUNT],
    system: Cell<Option<System>>,
    since: Cell<Option<Instant>>,
    suspended: Cell<bool>,
}

impl Clock {
    const fn new() -> Self {
        Self {
            totals: [const { Cell::new(0) }; SYSTEM_COUNT],
            system: Cell::new(None),
            since: Cell::new(None),
            suspended: Cell::new(false),
        }
    }

    fn charge(&self, now: Instant) {
        if !self.suspended.get() {
            if let (Some(system), Some(since)) = (self.system.get(), self.since.get()) {
                let total = &self.totals[system as usize];
                total.set(total.get().saturating_add(nanos(now.duration_since(since))));
            }
        }
        self.since.set(Some(now));
    }
}

thread_local! {
    static CLOCK: Clock = const { Clock::new() };
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Charges a scope to one system, excluding nested scopes in other systems.
/// Nested scopes in the same system perform no clock reads. Timings are
/// independent of the optional event trace and never allocate or lock.
pub struct SystemScope {
    previous: ScopeState,
    _thread: PhantomData<Rc<()>>,
}

enum ScopeState {
    Unchanged,
    Restore(Option<System>),
}

impl SystemScope {
    /// Begin an exclusive scope on the calling thread.
    #[must_use]
    pub fn new(system: System) -> Self {
        let previous = CLOCK.with(|clock| {
            if clock.suspended.get() || clock.system.get() == Some(system) {
                return ScopeState::Unchanged;
            }
            clock.charge(Instant::now());
            ScopeState::Restore(clock.system.replace(Some(system)))
        });
        Self {
            previous,
            _thread: PhantomData,
        }
    }
}

impl Drop for SystemScope {
    fn drop(&mut self) {
        if let ScopeState::Restore(previous) = self.previous {
            CLOCK.with(|clock| {
                clock.charge(Instant::now());
                clock.system.set(previous);
            });
        }
    }
}

/// Run an uninstrumented baseline for overhead calibration. Normal runtime
/// execution always records timings. Suspension is thread-local and restored
/// even if the calibration closure unwinds.
pub fn with_timing_suspended<T>(work: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            CLOCK.with(|clock| {
                if !self.0 {
                    clock.suspended.set(false);
                    clock.since.set(Some(Instant::now()));
                }
            });
        }
    }
    let previous = CLOCK.with(|clock| {
        let previous = clock.suspended.get();
        if !previous {
            clock.charge(Instant::now());
            clock.suspended.set(true);
        }
        previous
    });
    let _restore = Restore(previous);
    work()
}

/// Cumulative main-thread counters at a frame boundary. Snapshots can overlap:
/// Game measures its tick while the host measures the whole presentation frame.
pub struct FrameSnapshot {
    totals: [u64; SYSTEM_COUNT],
    start: Instant,
    _thread: PhantomData<Rc<()>>,
}

impl FrameSnapshot {
    /// Begin a runtime frame unless an overhead calibration suspended timing.
    #[must_use]
    pub fn when_recording() -> Option<Self> {
        CLOCK.with(|clock| !clock.suspended.get()).then(Self::now)
    }

    /// Capture the current counters without clearing another runtime's frame.
    #[must_use]
    pub fn now() -> Self {
        let start = Instant::now();
        let totals = CLOCK.with(|clock| {
            clock.charge(start);
            std::array::from_fn(|index| clock.totals[index].get())
        });
        Self {
            totals,
            start,
            _thread: PhantomData,
        }
    }

    /// Capture elapsed wall time and exclusive system deltas since this boundary.
    #[must_use]
    pub fn finish(&self) -> FrameSample {
        let end = Self::now();
        FrameSample {
            frame_ns: nanos(end.start.duration_since(self.start)),
            system_ns: std::array::from_fn(|index| {
                end.totals[index].saturating_sub(self.totals[index])
            }),
        }
    }
}

/// One completed frame. Headless frame time is tick wall time, while the host
/// uses time between presentation callbacks, including idle and rendering waits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameSample {
    pub frame_ns: u64,
    pub system_ns: [u64; SYSTEM_COUNT],
}

impl FrameSample {
    /// System duration in milliseconds.
    #[must_use]
    pub fn milliseconds(&self, system: System) -> f64 {
        self.system_ns[system as usize] as f64 / 1_000_000.0
    }

    /// System share of this frame's wall time. Empty frames have zero shares.
    #[must_use]
    pub fn percent(&self, system: System) -> f64 {
        if self.frame_ns == 0 {
            0.0
        } else {
            self.system_ns[system as usize] as f64 * 100.0 / self.frame_ns as f64
        }
    }
}

/// Rolling nearest-rank durations in nanoseconds, including idle zero samples.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SystemStats {
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub worst_ns: u64,
}

/// Fixed-capacity history with allocation-free frame recording and session totals.
pub struct FrameHistory {
    frames: Box<[FrameSample; FRAME_HISTORY]>,
    next: usize,
    count: usize,
    session_frames: u64,
    total_ns: [u64; SYSTEM_COUNT],
    worst_ns: [u64; SYSTEM_COUNT],
}

impl Default for FrameHistory {
    fn default() -> Self {
        Self {
            frames: Box::new(std::array::from_fn(|_| FrameSample::default())),
            next: 0,
            count: 0,
            session_frames: 0,
            total_ns: [0; SYSTEM_COUNT],
            worst_ns: [0; SYSTEM_COUNT],
        }
    }
}

impl FrameHistory {
    /// Append one completed frame in constant time.
    pub fn record(&mut self, sample: FrameSample) {
        for system in System::ALL {
            let index = system as usize;
            self.total_ns[index] = self.total_ns[index].saturating_add(sample.system_ns[index]);
            self.worst_ns[index] = self.worst_ns[index].max(sample.system_ns[index]);
        }
        self.frames[self.next] = sample;
        self.next = (self.next + 1) % FRAME_HISTORY;
        self.count = (self.count + 1).min(FRAME_HISTORY);
        self.session_frames += 1;
    }

    /// Most recently completed frame, or an empty sample before the first tick.
    #[must_use]
    pub fn last(&self) -> &FrameSample {
        &self.frames[(self.next + FRAME_HISTORY - 1) % FRAME_HISTORY]
    }

    /// Number of completed frames in this session.
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.session_frames
    }

    /// Compute rolling p50, p99 and worst for one system outside the tick.
    #[must_use]
    pub fn statistics(&self, system: System) -> SystemStats {
        if self.count == 0 {
            return SystemStats::default();
        }
        let mut values = [0; FRAME_HISTORY];
        for (index, frame) in self.frames.iter().take(self.count).enumerate() {
            values[index] = frame.system_ns[system as usize];
        }
        let values = &mut values[..self.count];
        values.sort_unstable();
        SystemStats {
            p50_ns: values[self.count.div_ceil(2) - 1],
            p99_ns: values[(self.count * 99).div_ceil(100) - 1],
            worst_ns: values[self.count - 1],
        }
    }

    /// Session total, mean and worst, plus rolling percentiles, for every system.
    #[must_use]
    pub fn summary_text(&self) -> String {
        let mut text = format!(
            "SOE systems: {} frames, rolling window {}\n",
            self.session_frames, self.count
        );
        let _ = writeln!(
            text,
            "system total_ms mean_ms rolling_p50_ms rolling_p99_ms worst_ms"
        );
        for system in System::ALL {
            let index = system as usize;
            let stats = self.statistics(system);
            let _ = writeln!(
                text,
                "{} {:.6} {:.6} {:.6} {:.6} {:.6}",
                system.name(),
                self.total_ns[index] as f64 / 1e6,
                self.total_ns[index] as f64 / self.session_frames.max(1) as f64 / 1e6,
                stats.p50_ns as f64 / 1e6,
                stats.p99_ns as f64 / 1e6,
                self.worst_ns[index] as f64 / 1e6
            );
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_12_every_system_reports_without_event_tracing() {
        let frame = FrameSnapshot::now();
        for system in System::ALL {
            let _scope = SystemScope::new(system);
            std::hint::black_box(system);
        }
        let mut history = FrameHistory::default();
        history.record(frame.finish());
        for system in System::ALL {
            assert!(
                history.last().system_ns[system as usize] > 0,
                "{}",
                system.name()
            );
            assert!(history.summary_text().contains(system.name()));
        }
    }

    #[test]
    fn prf_12_nested_scopes_have_exclusive_shares_and_snapshots_do_not_reset() {
        let frame = FrameSnapshot::now();
        {
            let _outer = SystemScope::new(System::Papyrus);
            let inner = FrameSnapshot::now();
            {
                let _same = SystemScope::new(System::Papyrus);
                let _native = SystemScope::new(System::Magic);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(inner.finish().system_ns[System::Magic as usize] > 0);
        }
        let sample = frame.finish();
        assert!(sample.system_ns.iter().sum::<u64>() <= sample.frame_ns);
        assert!(sample.milliseconds(System::Magic) > 0.0);
        assert!(sample.percent(System::Magic) <= 100.0);
        assert_eq!(FrameSample::default().percent(System::Magic), 0.0);
    }

    #[test]
    fn prf_12_rolling_percentiles_include_idle_frames_and_evict_old_worst() {
        let mut history = FrameHistory::default();
        for value in 1..=FRAME_HISTORY as u64 {
            history.record(FrameSample {
                frame_ns: value * SYSTEM_COUNT as u64,
                system_ns: [value; SYSTEM_COUNT],
            });
        }
        for system in System::ALL {
            assert_eq!(
                history.statistics(system),
                SystemStats {
                    p50_ns: 120,
                    p99_ns: 238,
                    worst_ns: 240
                }
            );
        }
        for _ in 0..FRAME_HISTORY {
            history.record(FrameSample::default());
        }
        assert_eq!(history.frames(), 480);
        assert_eq!(history.statistics(System::Save), SystemStats::default());
        assert!(history.summary_text().contains("0.000240"));
    }
}
