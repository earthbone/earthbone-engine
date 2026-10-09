// SPDX-License-Identifier: GPL-3.0-or-later

//! Host scene subphases of PRF-12's exclusive Render slot. Snapshot accounting
//! follows soe-trace frame.rs and excludes nested simulation system scopes.

use soe_trace::{FrameHistory, FrameSample, FrameSnapshot, System};
use std::cell::RefCell;

pub(crate) const PHASES: [(&str, System); 6] = [
    ("reference_collision_sync", System::Physics),
    ("reference_change_sync", System::Streaming),
    ("node_transforms", System::Input),
    ("mesh_material_uploads", System::Save),
    ("rendering_server_calls", System::Render),
    ("host_scene_other", System::Other),
];

#[derive(Default)]
struct Clock {
    phase: Option<System>,
    since: Option<FrameSnapshot>,
    totals: FrameSample,
}

impl Clock {
    fn charge(&mut self) {
        if let (Some(phase), Some(since)) = (self.phase, &self.since) {
            self.totals.system_ns[phase as usize] +=
                since.finish().system_ns[System::Render as usize];
        }
        self.since = Some(FrameSnapshot::now());
    }
}

thread_local! {
    static CLOCK: RefCell<Clock> = RefCell::default();
}

pub(crate) struct Scope(Option<System>);

impl Scope {
    pub(crate) fn new(phase: System) -> Self {
        Self(CLOCK.with(|clock| {
            let mut clock = clock.borrow_mut();
            clock.charge();
            clock.phase.replace(phase)
        }))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        CLOCK.with(|clock| {
            let mut clock = clock.borrow_mut();
            clock.charge();
            clock.phase = self.0;
        });
    }
}

pub(crate) fn snapshot() -> FrameSample {
    CLOCK.with(|clock| {
        let mut clock = clock.borrow_mut();
        clock.charge();
        clock.totals.clone()
    })
}

pub(crate) fn elapsed(start: &FrameSample) -> FrameSample {
    let end = snapshot();
    let mut sample = FrameSample {
        frame_ns: 0,
        system_ns: std::array::from_fn(|i| end.system_ns[i].saturating_sub(start.system_ns[i])),
        ..FrameSample::default()
    };
    sample.system_ns[System::Magic as usize] = PHASES
        .iter()
        .filter(|(_, slot)| *slot != System::Render)
        .map(|(_, slot)| sample.system_ns[*slot as usize])
        .sum();
    sample
}

pub(crate) fn rendering_server<T>(call: impl FnOnce() -> T) -> T {
    let _phase = Scope::new(System::Render);
    call()
}

pub(crate) fn report(history: &FrameHistory, frame_ns: u64) -> godot::builtin::VarDictionary {
    use godot::prelude::*;
    let mut report = VarDictionary::new();
    for (name, slot) in PHASES
        .into_iter()
        .chain([("host_scene_cpu", System::Magic)])
    {
        let stats = history.statistics(slot);
        let ms = history.last().milliseconds(slot);
        report.set(
            name,
            &vdict! {
                "ms" => ms,
                "percent" => if frame_ns == 0 { 0.0 } else { ms * 1e8 / frame_ns as f64 },
                "p50_ms" => stats.p50_ns as f64 / 1e6,
                "p99_ms" => stats.p99_ns as f64 / 1e6,
                "worst_ms" => stats.worst_ns as f64 / 1e6,
            },
        );
    }
    report
}

pub(crate) fn summary(history: &FrameHistory) -> String {
    let mut summary = String::from(
        "SOE host scene phases: total_ms mean_ms rolling_p50_ms rolling_p99_ms worst_ms\n",
    );
    for line in history.summary_text().lines() {
        if let Some((name, slot)) = PHASES
            .into_iter()
            .chain([("host_scene_cpu", System::Magic)])
            .find(|(_, slot)| line.split_whitespace().next() == Some(slot.name()))
        {
            summary.push_str(name);
            summary.push_str(line.strip_prefix(slot.name()).unwrap_or_default());
            summary.push('\n');
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_34_scene_phases_exclude_simulation_and_do_not_double_count_server_calls() {
        let start = snapshot();
        let frame = FrameSnapshot::now();
        {
            let _render = soe_trace::SystemScope::new(System::Render);
            let _scene = Scope::new(System::Other);
            {
                let _sync = Scope::new(System::Streaming);
                rendering_server(|| std::hint::black_box(1));
            }
            {
                let _game = soe_trace::SystemScope::new(System::Magic);
                std::hint::black_box(2);
            }
        }
        let phases = elapsed(&start);
        let sum: u64 = PHASES
            .iter()
            .map(|(_, slot)| phases.system_ns[*slot as usize])
            .sum();
        assert!(sum <= frame.finish().system_ns[System::Render as usize]);
        assert_eq!(
            phases.system_ns[System::Magic as usize],
            sum - phases.system_ns[System::Render as usize]
        );
        assert!(phases.system_ns[System::Streaming as usize] > 0);
        assert!(phases.system_ns[System::Render as usize] > 0);
    }
}
