// SPDX-License-Identifier: GPL-3.0-or-later

//! The game clock. Real seconds drive the VM scheduler and `OnUpdate`
//! timers, game time runs `timescale` times faster and is what
//! `Utility.GetCurrentGameTime` reports in days, the game's own unit.

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
        let dt = real_seconds.max(0.0);
        self.real_seconds += dt;
        self.game_days += dt * self.timescale / 86_400.0;
    }

    pub fn game_hours(&self) -> f64 {
        self.game_days * 24.0
    }

    /// `Utility.GameTimeToString`: hours of game time as `D.HH:MM:SS`.
    pub fn format_game_time(hours: f32) -> String {
        let total = (f64::from(hours) * 3600.0).round().max(0.0) as u64;
        let days = total / 86_400;
        let h = (total % 86_400) / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        format!("{days}.{h:02}:{m:02}:{s:02}")
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
        assert_eq!(GameClock::format_game_time(25.5), "1.01:30:00");
        assert_eq!(GameClock::format_game_time(0.0), "0.00:00:00");
    }
}
