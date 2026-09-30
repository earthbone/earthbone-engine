// SPDX-License-Identifier: GPL-3.0-or-later

//! Player HUD values read from the shared Game and camera heading.
//!
//! CK `GetActorValue` and `GetActorValueMax` define the stat inputs. SkyUI's
//! `MeterWidget` consumes percentages and its compass follows player angle.

use soe_esm::GlobalFormId;
use soe_game::Game;

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
    /// Five-point compass tape centered on the player's heading.
    pub compass: String,
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

fn compass_tape(yaw: f32) -> String {
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
