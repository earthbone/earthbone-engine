// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient frame timing used by Papyrus Utility. The capture contract follows
//! the Creation Kit `Utility_Script` page and its shipped `Utility.psc` wrapper.

use std::collections::VecDeque;

const FRAME_HISTORY_LIMIT: usize = 512;
const CAPTURE_STRING_LIMIT: usize = 1_000;

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
