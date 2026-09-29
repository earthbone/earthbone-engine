// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded door fade policy for cold texture preparation.
//! OpenMW delays its loading screen for 50 ms before drawing it.

use std::time::Duration;

/// Timed Papyrus fade request, sharing the player's existing overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GameFade {
    delay: f32,
    duration: f32,
    elapsed: f32,
    fading_out: bool,
    black: bool,
}

impl GameFade {
    /// Install a timed fade. Invalid durations leave the active fade unchanged.
    pub(crate) fn request(
        &mut self,
        fading_out: bool,
        black: bool,
        delay: f32,
        duration: f32,
    ) -> bool {
        if !delay.is_finite() || !duration.is_finite() || delay < 0.0 || duration < 0.0 {
            return false;
        }
        self.elapsed = 0.0;
        self.delay = delay;
        self.duration = duration;
        self.fading_out = fading_out;
        self.black = black;
        true
    }

    /// Advance the transition and return overlay opacity.
    pub(crate) fn advance(&mut self, elapsed: f32) -> f32 {
        if elapsed.is_finite() && elapsed > 0.0 {
            self.elapsed += elapsed;
        }
        self.opacity()
    }

    /// Current overlay opacity.
    #[must_use]
    pub(crate) fn opacity(&self) -> f32 {
        if self.elapsed < self.delay {
            return if self.fading_out { 0.0 } else { 1.0 };
        }
        if self.duration == 0.0 {
            return if self.fading_out { 1.0 } else { 0.0 };
        }
        let t = ((self.elapsed - self.delay) / self.duration).clamp(0.0, 1.0);
        if self.fading_out {
            t
        } else {
            1.0 - t
        }
    }

    #[must_use]
    pub(crate) fn is_black(&self) -> bool {
        self.black
    }
}

/// Cold texture preparation may consume this much time without a fade.
pub const COLD_TEXTURE_PREPARATION_BUDGET: Duration = Duration::from_millis(50);

/// A triggered fallback fades from black within this interval.
pub const DOOR_FADE_OUT: Duration = Duration::from_millis(150);

/// State for one fixed door fade overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoorFade {
    remaining: Duration,
    last_preparation: Duration,
    activations: u64,
}

impl DoorFade {
    /// Record completed texture preparation and answer whether it exceeded the budget.
    pub fn record_preparation(&mut self, elapsed: Duration) -> bool {
        self.last_preparation = elapsed;
        if elapsed > COLD_TEXTURE_PREPARATION_BUDGET {
            self.remaining = DOOR_FADE_OUT;
            self.activations = self.activations.saturating_add(1);
            true
        } else {
            self.remaining = Duration::ZERO;
            false
        }
    }

    /// Advance the fade and return its current opacity in the range zero to one.
    pub fn advance(&mut self, elapsed: Duration) -> f32 {
        self.remaining = self.remaining.saturating_sub(elapsed);
        self.opacity()
    }

    /// Current overlay opacity in the range zero to one.
    #[must_use]
    pub fn opacity(&self) -> f32 {
        self.remaining.as_secs_f32() / DOOR_FADE_OUT.as_secs_f32()
    }

    /// Whether the overlay should be visible.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.remaining.is_zero()
    }

    /// Most recent texture preparation time.
    #[must_use]
    pub fn last_preparation(&self) -> Duration {
        self.last_preparation
    }

    /// Number of preparations that exceeded the budget.
    #[must_use]
    pub fn activations(&self) -> u64 {
        self.activations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_activates_only_above_the_declared_budget() {
        let mut fade = DoorFade::default();

        let just_below = Duration::from_millis(49) + Duration::from_nanos(999_999);
        assert!(!fade.record_preparation(just_below));
        assert!(!fade.record_preparation(COLD_TEXTURE_PREPARATION_BUDGET));
        assert!(fade.record_preparation(COLD_TEXTURE_PREPARATION_BUDGET + Duration::from_nanos(1)));
        assert_eq!(fade.activations(), 1);
    }

    #[test]
    fn fade_reaches_clear_at_its_declared_bound() {
        let mut fade = DoorFade::default();
        assert!(fade.record_preparation(COLD_TEXTURE_PREPARATION_BUDGET + Duration::from_nanos(1)));

        assert_eq!(fade.advance(DOOR_FADE_OUT / 2), 0.5);
        assert_eq!(fade.advance(DOOR_FADE_OUT / 2), 0.0);
        assert!(!fade.is_active());
    }

    #[test]
    fn scr_28_fade_obeys_delay_duration_direction_and_validation() {
        let mut fade = GameFade::default();
        assert!(!fade.request(true, true, -1.0, 1.0));
        assert!(fade.request(true, true, 0.5, 2.0));
        assert_eq!(fade.advance(0.5), 0.0);
        assert_eq!(fade.advance(1.0), 0.5);
        assert_eq!(fade.advance(1.0), 1.0);
        assert!(fade.is_black());
        assert!(fade.request(false, false, 0.0, 1.0));
        assert_eq!(fade.advance(0.5), 0.5);
        assert_eq!(fade.advance(0.5), 0.0);
        assert!(!fade.is_black());
    }
}
