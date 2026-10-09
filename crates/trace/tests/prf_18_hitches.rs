// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-18 synthetic ticks through the public frame and work capture APIs.
//! Acceptance windows and overhead bound come from docs/performance.md.

use soe_trace::{
    count, FrameHistory, FrameSnapshot, Operation, System, SystemScope, Trace, WorkFrame,
};
use std::time::{Duration, Instant};

fn tick(history: &mut FrameHistory, delay: Duration) {
    let frame = FrameSnapshot::now();
    let _work = WorkFrame::begin();
    {
        let _span = soe_trace::Span::with_payload("synthetic.tick", "test", 42);
        let _dynamic = soe_trace::Span::dynamic(
            || panic!("dynamic name allocated with tracing off"),
            "native",
        );
        let _system = SystemScope::new(System::Papyrus);
        count(Operation::PapyrusInstructions, 17);
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
    history.record(frame.finish());
}

#[test]
fn prf_18_slow_synthetic_tick_dumps_prior_frames_with_optional_tracing_off() {
    let directory = std::env::temp_dir().join(format!("prf-18-tick-{}", std::process::id()));
    let mut history = FrameHistory::default();
    Trace::set_hitch_directory(&directory);
    assert!(!Trace::enabled());
    for _ in 0..4 {
        tick(&mut history, Duration::ZERO);
    }
    tick(&mut history, Duration::from_millis(50));
    let tail = Instant::now();
    while tail.elapsed() < Duration::from_millis(1050) {
        tick(&mut history, Duration::ZERO);
        std::thread::sleep(Duration::from_millis(10));
    }
    Trace::wait_for_hitch_exports().unwrap();
    let path = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .unwrap();
    let json = std::fs::read_to_string(&path).unwrap();
    let trigger: f64 = json
        .split("\"trigger_us\":")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let frames: Vec<f64> = json
        .split("\"name\":\"frame\"")
        .skip(1)
        .map(|event| {
            event
                .split("\"ts\":")
                .nth(1)
                .unwrap()
                .split(',')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    assert!(frames.iter().filter(|&&ts| ts < trigger - 50_000.0).count() >= 4);
    assert!(frames.iter().any(|&ts| ts > trigger + 900_000.0));
    assert!(json.contains("\"name\":\"papyrus_vm\""));
    assert!(json.contains("\"name\":\"synthetic.tick\""));
    assert!(json.contains("\"name\":\"native\""));
    assert!(json.contains("\"payload\":42"));
    assert!(json.contains("\"name\":\"papyrus_instructions\""));
    assert!(json.contains("\"count\":17"));
    assert!(Trace::events().is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
#[ignore = "requires an optimized build and an idle machine"]
fn prf_18_recorder_stays_inside_one_percent_of_synthetic_tick() {
    const MAX_RATIO: f64 = 1.01;
    const PAIRS: usize = 31;
    fn batch(history: &mut FrameHistory) {
        for _ in 0..8 {
            let snapshot = FrameSnapshot::when_recording();
            for system in System::ALL {
                let _scope = SystemScope::new(system);
                let _span = soe_trace::Span::new("synthetic.system", "synthetic");
                let mut state = 7_u64;
                for _ in 0..40_000 {
                    state = std::hint::black_box(
                        state.wrapping_mul(6364136223846793005).wrapping_add(1),
                    );
                }
                std::hint::black_box(state);
            }
            if let Some(snapshot) = snapshot {
                history.record(snapshot.finish());
            }
        }
    }
    let mut history = FrameHistory::default();
    batch(&mut history);
    let mut ratios = Vec::new();
    for pair in 0..PAIRS {
        let mut times = [0.0; 2];
        for slot in if pair % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            if slot == 0 {
                batch(&mut history);
            } else {
                soe_trace::with_timing_suspended(|| batch(&mut history));
            }
            times[slot] = start.elapsed().as_secs_f64();
        }
        ratios.push(times[0] / times[1]);
    }
    ratios.sort_by(f64::total_cmp);
    let ratio = ratios[PAIRS / 2];
    eprintln!("PRF-18 synthetic tick on/off ratio {ratio:.6}");
    assert!(ratio < MAX_RATIO, "ratio {ratio} exceeds {MAX_RATIO}");
}
