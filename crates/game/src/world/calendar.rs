// SPDX-License-Identifier: GPL-3.0-or-later

//! Calendar globals follow CommonLibSSE-NG Calendar.h and Calendar.cpp.

use super::World;
use crate::clock::Calendar;
use soe_esm::{GlobalFormId, Tag};

impl World {
    /// Return the time-driven sun position in Skyrim world units. This
    /// engine model places the sun on a 100,000-unit orbit, east at 06:00,
    /// overhead at 12:00 and west at 18:00.
    pub fn sun_position(&self) -> [f32; 3] {
        let hour = self.calendar().hour.rem_euclid(24.0);
        let angle = (hour - 6.0) * std::f64::consts::PI / 12.0;
        let direction = [angle.cos(), 0.0, angle.sin()];
        let player = self.player.and_then(|id| self.refs.get(&id));
        let origin = player.map_or([0.0; 3], |r| r.position);
        [
            origin[0] + direction[0] as f32 * 100_000.0,
            origin[1],
            origin[2] + direction[2] as f32 * 100_000.0,
        ]
    }

    /// Whether the host camera points within one degree of the sun.
    pub fn player_is_sungazing(&self) -> bool {
        let sun = self.sun_position();
        let origin = self
            .player
            .and_then(|id| self.refs.get(&id))
            .map_or([0.0; 3], |r| r.position);
        let mut to_sun = [sun[0] - origin[0], sun[1] - origin[1], sun[2] - origin[2]];
        let sun_len = (to_sun.iter().map(|v| v * v).sum::<f32>()).sqrt();
        let look_len = (self.camera_look.iter().map(|v| v * v).sum::<f32>()).sqrt();
        if !sun_len.is_finite() || !look_len.is_finite() || sun_len == 0.0 || look_len == 0.0 {
            return false;
        }
        for value in &mut to_sun {
            *value /= sun_len;
        }
        let dot = to_sun
            .iter()
            .zip(self.camera_look)
            .map(|(a, b)| a * (b / look_len))
            .sum::<f32>();
        dot >= 0.999_847_7
    }

    /// Queue a host camera effect. Invalid strengths or durations are ignored.
    pub fn shake_camera(&mut self, source: GlobalFormId, strength: f32, duration: f32) {
        if !strength.is_finite() || !duration.is_finite() || duration < 0.0 {
            return;
        }
        self.camera_shakes.push(crate::world::CameraShake {
            source,
            strength: strength.clamp(0.0, 1.0),
            duration,
        });
    }

    pub(crate) fn calendar_global(&self, name: &str) -> Option<(GlobalFormId, f32)> {
        let id = self.form(name)?;
        let record = self.record(id)?;
        if record.tag != Tag(*b"GLOB") || record.is_deleted() {
            return None;
        }
        let value = self.globals.get(&id).copied().or_else(|| {
            let bytes: [u8; 4] = record.find(Tag(*b"FLTV"))?.try_into().ok()?;
            Some(f32::from_le_bytes(bytes))
        })?;
        Some((id, value))
    }

    /// Read the live date and hour without constructing mutable form state.
    pub fn calendar(&self) -> Calendar {
        let fallback = Calendar::from_clock(&self.clock);
        let value = |name: &str, default: f64| {
            self.calendar_global(name)
                .map_or(default, |(_, v)| f64::from(v))
        };
        Calendar {
            year: value("GameYear", f64::from(fallback.year)) as u32,
            month: value("GameMonth", f64::from(fallback.month)) as u32,
            day: value("GameDay", f64::from(fallback.day)) as u32,
            hour: value("GameHour", fallback.hour),
        }
    }

    /// Advance calendar globals before timers and packages observe this tick.
    pub fn advance_clock(&mut self, real_seconds: f64) {
        self.advance_clock_scaled(real_seconds, 1.0);
    }

    pub(crate) fn advance_clock_scaled(&mut self, real_seconds: f64, game_time_scale: f64) {
        let mut calendar = self.calendar();
        let elapsed_hours = real_seconds.max(0.0) * self.clock.timescale * game_time_scale / 3600.0;
        self.clock.advance_scaled(real_seconds, game_time_scale);
        if elapsed_hours == 0.0 {
            return;
        }
        let hours = calendar.hour + elapsed_hours;
        if calendar.month < 12 && calendar.day > 0 && hours.is_finite() && hours >= 0.0 {
            calendar.add_days((hours / 24.0).floor() as u64);
            calendar.hour = hours.rem_euclid(24.0);
            for (name, value) in [
                ("GameYear", calendar.year as f32),
                ("GameMonth", calendar.month as f32),
                ("GameDay", calendar.day as f32),
                ("GameHour", calendar.hour as f32),
                ("GameDaysPassed", self.clock.game_days as f32),
            ] {
                if let Some((id, _)) = self.calendar_global(name) {
                    self.globals.insert(id, value);
                }
            }
        }
    }
}
