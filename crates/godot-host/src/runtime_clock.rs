// SPDX-License-Identifier: GPL-3.0-or-later

//! Wall-time input for automatic gameplay. Godot class_engine.rst documents
//! that max_physics_steps_per_frame drops simulation time at low frame rates.
//! The existing capsule controller substeps movement without repeating the VM.

use std::time::Instant;

#[derive(Default)]
pub(crate) struct RuntimeClock {
    previous: Option<Instant>,
}

impl RuntimeClock {
    pub(crate) fn sample(&mut self, now: Instant, active: bool, fallback: f64) -> f64 {
        let previous = self.previous.take();
        if !active {
            return fallback;
        }
        self.previous = Some(now);
        previous.map_or(fallback, |previous| {
            now.saturating_duration_since(previous).as_secs_f64()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn capped_physics_callback_keeps_wall_time_at_twenty_fps() {
        let start = Instant::now();
        let mut clock = RuntimeClock::default();
        clock.sample(start, true, 1.0 / 60.0);
        let seconds: f64 = (1..=200)
            .map(|frame| clock.sample(start + Duration::from_millis(frame * 50), true, 1.0 / 60.0))
            .sum();
        assert!((seconds - 10.0).abs() < 1e-10, "{seconds}");
    }

    #[test]
    fn paused_loading_and_manual_intervals_do_not_build_time_debt() {
        let start = Instant::now();
        let mut clock = RuntimeClock::default();
        clock.sample(start, true, 1.0 / 60.0);
        clock.sample(start + Duration::from_secs(1), false, 1.0 / 60.0);
        let resumed = clock.sample(start + Duration::from_secs(21), true, 1.0 / 60.0);
        assert_eq!(resumed, 1.0 / 60.0);
        let next = clock.sample(start + Duration::from_millis(21050), true, 1.0 / 60.0);
        assert!((next - 0.05).abs() < 1e-12);
    }
}
