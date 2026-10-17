// SPDX-License-Identifier: GPL-3.0-or-later

//! Immersion follows OpenMW's actor-height water test. The host supplies the
//! CELL/WRLD height, the race head height and the loaded fSwimHeightScale.

pub(crate) fn submerged(
    was_swimming: bool,
    feet: f32,
    water: Option<f32>,
    height: f32,
    fraction: f32,
) -> bool {
    let Some(water) = water.filter(|water| water.is_finite()) else {
        return false;
    };
    if !feet.is_finite() || !height.is_finite() || height <= 0.0 {
        return false;
    }
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.3, 0.95)
    } else {
        0.65
    };
    let depth = height * fraction * if was_swimming { 0.4 } else { 1.0 };
    feet + depth < water
}

pub(crate) fn surface_depth(height: f32, posed_head: Option<f32>, fraction: f32) -> f32 {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.3, 0.95)
    } else {
        0.65
    };
    let initial = height * fraction;
    posed_head.map_or(initial, |head| {
        (head - height * 0.065).clamp(initial * 0.6, initial * 1.3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r4_swim_entry_uses_actor_height_and_surface_hold_has_hysteresis() {
        assert!(submerged(false, 0.0, Some(80.0), 120.0, 0.65));
        assert!(!submerged(false, 0.0, Some(80.0), 180.0, 0.65));
        assert!(!submerged(false, 50.0, Some(80.0), 120.0, 0.65));
        assert!(submerged(true, 40.0, Some(80.0), 120.0, 0.65));
        assert!(!submerged(true, 55.0, Some(80.0), 120.0, 0.65));
        assert!(!submerged(true, 0.0, None, 120.0, 0.65));
    }

    #[test]
    fn r4_swim_surface_target_retains_head_clearance_without_crossing_exit_depth() {
        let depth = surface_depth(120.0, Some(64.0), 0.65);
        assert!((64.0 - depth - 7.8).abs() < 0.001);
        assert!(submerged(true, 100.0 - depth, Some(100.0), 120.0, 0.65));
        assert_eq!(surface_depth(120.0, None, 0.65), 78.0);
    }
}
