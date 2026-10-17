// SPDX-License-Identifier: GPL-3.0-or-later

//! Player HUD values read from the shared Game and camera heading.
//!
//! CK `GetActorValue` and `GetActorValueMax` define the stat inputs. SkyUI's
//! `MeterWidget` consumes percentages and its compass follows player angle.

use soe_esm::GlobalFormId;
use soe_game::Game;

/// Transient visible state for Game natives presented by the player HUD.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GameUiPresentation {
    pub cart_mode: bool,
    pub blood_alpha: f32,
    blood_remaining: f32,
}

impl GameUiPresentation {
    /// Copy the authoritative HUD mode from the runtime world.
    pub fn set_cart_mode(&mut self, enabled: bool) {
        self.cart_mode = enabled;
    }

    /// Start the short blood overlay using the native's signed intensity.
    pub fn trigger_screen_blood(&mut self, value: i32) {
        self.blood_alpha = value.unsigned_abs().min(100) as f32 / 100.0;
        self.blood_remaining = if self.blood_alpha > 0.0 { 0.45 } else { 0.0 };
    }

    /// Advance the overlay timer and clear it when the effect ends.
    pub fn tick(&mut self, delta: f64) {
        self.blood_remaining = (self.blood_remaining - delta.max(0.0) as f32).max(0.0);
        if self.blood_remaining == 0.0 {
            self.blood_alpha = 0.0;
        }
    }
}

const DIRECTIONS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// One actor-value meter as displayed by the player HUD.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// Current actor value.
    pub current: f32,
    /// Maximum actor value after modifiers.
    pub maximum: f32,
    /// Current value divided by maximum, clamped for presentation.
    pub fraction: f32,
}

/// Current player HUD data, derived from Game and the camera's Skyrim yaw.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Player health meter.
    pub health: Meter,
    /// Player magicka meter.
    pub magicka: Meter,
    /// Player stamina meter.
    pub stamina: Meter,
    /// Nearby headings for accessibility and diagnostics.
    pub compass: String,
    /// Horizontal position of the repeated compass strip, in pixels.
    pub compass_offset: f32,
}

impl Snapshot {
    /// Read the three player meters from the shared Game and compass heading.
    pub fn read(game: &mut Game, player: GlobalFormId, yaw: f32) -> Self {
        let mut world = game.world_mut();
        Self {
            health: read_meter(&mut world, player, "health"),
            magicka: read_meter(&mut world, player, "magicka"),
            stamina: read_meter(&mut world, player, "stamina"),
            compass: compass_tape(yaw),
            compass_offset: compass_offset(yaw),
        }
    }
}

fn read_meter(world: &mut soe_game::World, player: GlobalFormId, name: &str) -> Meter {
    let current = world.actor_value(player, name);
    let maximum = world.actor_value_maximum(player, name).max(0.0);
    let fraction = if maximum > 0.0 {
        (current / maximum).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Meter {
        current,
        maximum,
        fraction,
    }
}

/// The 720 pixel compass period moves left by two pixels per yaw degree.
#[must_use]
pub fn compass_offset(yaw: f32) -> f32 {
    if yaw.is_finite() {
        -yaw.rem_euclid(std::f32::consts::TAU).to_degrees() * 2.0
    } else {
        0.0
    }
}

pub(crate) fn compass_tape(yaw: f32) -> String {
    let yaw = if yaw.is_finite() { yaw } else { 0.0 };
    let direction = (yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::FRAC_PI_4).round()
        as usize
        % DIRECTIONS.len();
    (-2_isize..=2)
        .map(|offset| {
            let index =
                (direction as isize + offset).rem_euclid(DIRECTIONS.len() as isize) as usize;
            if offset == 0 {
                format!("[{}]", DIRECTIONS[index])
            } else {
                DIRECTIONS[index].to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("   ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_29_camera_degree_steps_move_the_strip_continuously() {
        const TOLERANCE: f32 = 0.001;
        let mut camera = crate::camera::CameraRig::default();
        let mut previous = 0.0;
        for degree in 0..=720 {
            camera.set_facing((degree as f32).to_radians(), 0.0);
            let current = compass_offset(camera.yaw());
            if degree > 0 {
                let step = (current - previous).rem_euclid(720.0);
                assert!((step - 718.0).abs() <= TOLERANCE, "degree {degree}: {step}");
            }
            previous = current;
        }
        assert_eq!(compass_offset(f32::NAN), 0.0);
        assert_eq!(compass_offset(f32::INFINITY), 0.0);
    }

    #[test]
    fn ui_06_compass_tape_tracks_heading_and_wraps() {
        assert_eq!(compass_tape(0.0), "W   NW   [N]   NE   E");
        assert_eq!(
            compass_tape(std::f32::consts::FRAC_PI_2),
            "N   NE   [E]   SE   S"
        );
        assert_eq!(compass_tape(std::f32::consts::TAU), "W   NW   [N]   NE   E");
        assert_eq!(compass_tape(f32::NAN), "W   NW   [N]   NE   E");
    }
}
