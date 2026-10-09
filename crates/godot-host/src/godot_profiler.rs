// SPDX-License-Identifier: GPL-3.0-or-later

//! Publish engine system timings through Godot's EngineProfiler debugger API.

use godot::classes::{EngineDebugger, EngineProfiler, IEngineProfiler};
use godot::prelude::*;
use soe_trace::{FrameHistory, FrameSample, System};

const PROFILER_NAME: &str = "soe_systems";

#[derive(GodotClass)]
#[class(base = EngineProfiler)]
struct SoeSystemsProfiler {
    base: Base<EngineProfiler>,
    enabled: bool,
    frames: u64,
}

#[godot_api]
impl IEngineProfiler for SoeSystemsProfiler {
    fn init(base: Base<EngineProfiler>) -> Self {
        Self {
            base,
            enabled: false,
            frames: 0,
        }
    }

    fn toggle(&mut self, enable: bool, _options: VarArray) {
        self.enabled = enable;
        if !enable {
            self.frames = 0;
        }
    }

    fn add_frame(&mut self, data: VarArray) {
        if self.enabled {
            self.frames = self.frames.saturating_add(1);
            let _ = data;
        }
    }

    fn tick(
        &mut self,
        _frame_time: f64,
        _process_time: f64,
        _physics_time: f64,
        _physics_frame_time: f64,
    ) {
    }
}

/// Register the runtime profiler so the editor debugger can enable it.
pub(crate) fn register() {
    let mut debugger = EngineDebugger::singleton();
    if !debugger.has_profiler(PROFILER_NAME) {
        let profiler = SoeSystemsProfiler::new_gd();
        debugger.register_profiler(PROFILER_NAME, &profiler);
    }
}

/// Remove the profiler before the GDExtension unloads its class callbacks.
pub(crate) fn unregister() {
    let mut debugger = EngineDebugger::singleton();
    if debugger.has_profiler(PROFILER_NAME) {
        debugger.unregister_profiler(PROFILER_NAME);
    }
}

/// Submit one completed PlayerNode frame as name and duration pairs in seconds.
pub(crate) fn publish(history: &FrameHistory) {
    let name = StringName::from(PROFILER_NAME);
    let mut debugger = EngineDebugger::singleton();
    if !debugger.is_profiling(&name) {
        return;
    }

    let sample = history.last();
    let mut data = VarArray::new();
    data.push(sample.frame_ns as f64 / 1_000_000_000.0);
    for (system, seconds) in system_frame_values(sample) {
        data.push(&GString::from(system));
        data.push(seconds);
    }
    debugger.profiler_add_frame_data(&name, &data);
}

fn system_frame_values(sample: &FrameSample) -> Vec<(&'static str, f64)> {
    System::ALL
        .into_iter()
        .map(|system| {
            (
                system.name(),
                sample.system_ns[system as usize] as f64 / 1_000_000_000.0,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_trace::SYSTEM_COUNT;

    #[test]
    fn prf_23_editor_profiler_payload_contains_every_system_in_seconds() {
        let sample = FrameSample {
            frame_ns: 1_000_000_000,
            system_ns: std::array::from_fn(|index| (index as u64 + 1) * 1_000_000),
            live_bytes: [0; SYSTEM_COUNT],
            allocation_calls: [0; SYSTEM_COUNT],
        };
        let values = system_frame_values(&sample);
        assert_eq!(values.len(), System::ALL.len());
        for (index, (name, seconds)) in values.into_iter().enumerate() {
            assert_eq!(name, System::ALL[index].name());
            assert!((seconds - (index as f64 + 1.0) / 1_000.0).abs() < f64::EPSILON);
        }
    }
}
