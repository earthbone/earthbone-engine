// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient frame timing used by Papyrus Utility. The capture contract follows
//! the Creation Kit `Utility_Script` page and its shipped `Utility.psc` wrapper.

use std::collections::VecDeque;

const FRAME_HISTORY_LIMIT: usize = 512;
const CAPTURE_STRING_LIMIT: usize = 1_000;

/// Papyrus-facing view of the host's per-system Rust allocation counters.
#[derive(Debug, Default)]
pub(crate) struct MemoryTracker {
    initialized: bool,
}

impl MemoryTracker {
    /// Enable memory queries and return the current requested Rust allocation
    /// bytes across the host's tracked systems.
    pub(crate) fn current_memory(&mut self) -> i32 {
        self.initialized = true;
        let snapshot = soe_trace::MemorySnapshot::now();
        let bytes = snapshot.live_bytes.iter().copied().sum::<u64>();
        i32::try_from(bytes).unwrap_or(i32::MAX)
    }

    pub(crate) fn budget_count(&self) -> Option<i32> {
        self.initialized
            .then(|| i32::try_from(soe_trace::SYSTEM_COUNT).unwrap_or(i32::MAX))
    }

    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub(crate) fn budget_name(&self, index: i32) -> Option<&'static str> {
        if !self.initialized {
            return None;
        }
        usize::try_from(index)
            .ok()
            .and_then(|index| soe_trace::System::ALL.get(index))
            .map(|system| system.name())
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrameRateState {
    recent: VecDeque<f32>,
    capture: Vec<f32>,
    capturing: bool,
}

impl FrameRateState {
    pub(crate) fn record_frame(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let fps = (1.0 / seconds) as f32;
        if !fps.is_finite() {
            return;
        }
        if self.recent.len() == FRAME_HISTORY_LIMIT {
            self.recent.pop_front();
        }
        self.recent.push_back(fps);
        if self.capturing {
            self.capture.push(fps);
        }
    }

    pub(crate) fn start(&mut self) -> Result<(), &'static str> {
        if self.capturing {
            return Err("Utility.StartFrameRateCapture called while capture is active");
        }
        self.capture.clear();
        self.capturing = true;
        Ok(())
    }

    pub(crate) fn end(&mut self) -> Result<(), &'static str> {
        if !self.capturing {
            return Err("Utility.EndFrameRateCapture called without an active capture");
        }
        self.capturing = false;
        Ok(())
    }

    pub(crate) fn average(&self) -> Result<f32, &'static str> {
        if self.capture.is_empty() {
            return Err("Utility.GetAverageFrameRate has no captured frames");
        }
        Ok(self.capture.iter().sum::<f32>() / self.capture.len() as f32)
    }

    pub(crate) fn minimum(&self) -> Result<f32, &'static str> {
        self.capture
            .iter()
            .copied()
            .reduce(f32::min)
            .ok_or("Utility.GetMinFrameRate has no captured frames")
    }

    pub(crate) fn maximum(&self) -> Result<f32, &'static str> {
        self.capture
            .iter()
            .copied()
            .reduce(f32::max)
            .ok_or("Utility.GetMaxFrameRate has no captured frames")
    }

    pub(crate) fn capture_recent(&self, frame_count: i32) -> Result<String, &'static str> {
        if frame_count <= 0 {
            return Err("Utility.CaptureFrameRate requires a positive frame count");
        }
        let frame_count = frame_count as usize;
        if frame_count > self.recent.len() {
            return Err("Utility.CaptureFrameRate has fewer recent frames than requested");
        }
        let start = self.recent.len() - frame_count;
        let output = self
            .recent
            .iter()
            .skip(start)
            .map(|fps| format!("{fps:.2}"))
            .collect::<Vec<_>>()
            .join(",");
        if output.len() > CAPTURE_STRING_LIMIT {
            return Err("Utility.CaptureFrameRate output exceeds 1,000 characters");
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use crate::world::World;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

    #[test]
    fn prf_20_navigation_load_time_is_cell_owned_transient_and_consumed_once() {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag::new(b"CELL"), 1).sub(b"DATA", [1, 0]));
        plugin.push_top(Record::new(Tag::new(b"CELL"), 2).sub(b"DATA", [1, 0]));
        let mut order = LoadOrder::new();
        order.add("Test.esm", plugin).unwrap();
        let mut world = World::new(order);
        let a = GlobalFormId { file: 0, object: 1 };
        let b = GlobalFormId { file: 0, object: 2 };
        assert_eq!(world.take_navigation_load_time(a), 0);
        world.load_cell(a);
        world.load_cell(b);
        assert!(world.take_navigation_load_time(a) > 0);
        assert_eq!(world.take_navigation_load_time(a), 0);
        assert!(world.take_navigation_load_time(b) > 0);
    }
}
