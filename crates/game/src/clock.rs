// SPDX-License-Identifier: GPL-3.0-or-later

//! The game clock. Real seconds drive the VM scheduler and `OnUpdate`
//! timers, game time runs `timescale` times faster and is what
//! `Utility.GetCurrentGameTime` reports in days, the game's own unit.
//! Calendar fields and month lengths follow CommonLibSSE-NG Calendar.h/.cpp.

/// Calendar globals are separate values, including hour and elapsed days.
pub const CALENDAR_GLOBALS: [&str; 6] = [
    "GameYear",
    "GameMonth",
    "GameDay",
    "GameHour",
    "GameDaysPassed",
    "TimeScale",
];

pub(crate) const DAYS_IN_MONTH: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A read-only view of the current form-backed calendar. Months are zero based.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calendar {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: f64,
}

impl Calendar {
    pub(crate) fn from_clock(clock: &GameClock) -> Self {
        let mut calendar = Self {
            year: 201,
            month: 7,
            day: 17,
            hour: clock.game_hours().rem_euclid(24.0),
        };
        calendar.add_days(clock.game_days.floor().max(0.0) as u64);
        calendar
    }

    pub(crate) fn add_days(&mut self, days: u64) {
        let mut day = u64::from(self.day) + days % 365;
        self.year = self
            .year
            .saturating_add((days / 365).min(u64::from(u32::MAX)) as u32);
        while day > u64::from(DAYS_IN_MONTH[self.month as usize]) {
            day -= u64::from(DAYS_IN_MONTH[self.month as usize]);
            self.month += 1;
            if self.month == 12 {
                self.month = 0;
                self.year = self.year.saturating_add(1);
            }
        }
        self.day = day as u32;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameClock {
    pub real_seconds: f64,
    /// Days of game time passed, the value `GetCurrentGameTime` returns.
    pub game_days: f64,
    /// Game seconds per real second. Skyrim ships with 20.
    pub timescale: f64,
}

impl Default for GameClock {
    fn default() -> Self {
        GameClock {
            real_seconds: 0.0,
            game_days: 0.0,
            timescale: 20.0,
        }
    }
}

impl GameClock {
    pub fn advance(&mut self, real_seconds: f64) {
        self.advance_scaled(real_seconds, 1.0);
    }

    pub(crate) fn advance_scaled(&mut self, real_seconds: f64, game_time_scale: f64) {
        let dt = real_seconds.max(0.0);
        self.real_seconds += dt;
        self.game_days += dt * self.timescale * game_time_scale / 86_400.0;
    }

    pub fn game_hours(&self) -> f64 {
        self.game_days * 24.0
    }

    /// `Utility.GameTimeToString`: game days as a calendar date and time.
    pub fn format_game_time(game_days: f32) -> String {
        let elapsed = f64::from(game_days).max(0.0);
        let mut calendar = Calendar {
            year: 201,
            month: 7,
            day: 17,
            hour: elapsed.fract() * 24.0,
        };
        calendar.add_days(elapsed.floor().min(u64::MAX as f64) as u64);
        let hour = calendar.hour.floor() as u32;
        let minute = (calendar.hour.fract() * 60.0).floor() as u32;
        format!(
            "{:02}/{:02}/{:03} {hour:02}:{minute:02}",
            calendar.month + 1,
            calendar.day,
            calendar.year
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_time_runs_at_timescale() {
        let mut c = GameClock::default();
        c.advance(4320.0);
        assert!((c.game_days - 1.0).abs() < 1e-9);
        assert!((c.game_hours() - 24.0).abs() < 1e-9);
        c.advance(-5.0);
        assert_eq!(c.real_seconds, 4320.0);
    }

    #[test]
    fn game_time_formats_like_the_game() {
        assert_eq!(GameClock::format_game_time(0.0), "08/17/201 00:00");
        assert_eq!(GameClock::format_game_time(0.5), "08/17/201 12:00");
        assert_eq!(GameClock::format_game_time(1.0), "08/18/201 00:00");
    }

    #[test]
    fn scr_72_game_time_to_string_uses_game_days_and_calendar_time() {
        assert_eq!(GameClock::format_game_time(2.25), "08/19/201 06:00");
    }
}
