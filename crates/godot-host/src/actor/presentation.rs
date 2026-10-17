// SPDX-License-Identifier: GPL-3.0-or-later

//! Render snapshots follow Godot's physics_interpolation_introduction.rst.
//! Locomotion uses simulation intervals, while the displayed placement uses
//! the host's interpolation fraction. Neither snapshot changes World.

use soe_game::world::RefPlacement;

use super::NpcMotion;

#[derive(Clone, Copy)]
struct Sample {
    placement: RefPlacement,
    seconds: f64,
}

#[derive(Default)]
pub(crate) struct NpcPresentation {
    previous: Option<Sample>,
    current: Option<Sample>,
    motion: Option<NpcMotion>,
}

impl NpcPresentation {
    pub(crate) fn sample(
        &mut self,
        placement: RefPlacement,
        seconds: f64,
        fraction: f32,
    ) -> (RefPlacement, Option<NpcMotion>) {
        if !seconds.is_finite() {
            return (placement, None);
        }
        let next = Sample { placement, seconds };
        match self.current {
            Some(current) if seconds == current.seconds && placement == current.placement => {}
            Some(current)
                if seconds > current.seconds
                    && separation(current.placement.position, placement.position)
                        <= crate::npc_grounding::SPAWN_SNAP =>
            {
                self.previous = Some(current);
                self.current = Some(next);
                self.motion = NpcMotion::between(
                    current.placement,
                    placement,
                    (seconds - current.seconds) as f32,
                );
            }
            _ => {
                self.previous = Some(next);
                self.current = Some(next);
                self.motion = NpcMotion::between(placement, placement, 1.0);
            }
        }
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let Some(previous) = self.previous else {
            return (placement, self.motion);
        };
        let mut rendered = placement;
        rendered.position = std::array::from_fn(|axis| {
            previous.placement.position[axis]
                + (placement.position[axis] - previous.placement.position[axis]) * fraction
        });
        rendered.rotation = std::array::from_fn(|axis| {
            let difference = (placement.rotation[axis] - previous.placement.rotation[axis]
                + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            previous.placement.rotation[axis] + difference * fraction
        });
        (rendered, self.motion)
    }
}

fn separation(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1]).hypot(a[2] - b[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(x: f32) -> RefPlacement {
        RefPlacement {
            position: [x, 0.0, 0.0],
            rotation: [0.0; 3],
            scale: 1.0,
            visible: true,
        }
    }

    #[test]
    fn render_frames_between_physics_ticks_keep_locomotion_and_interpolate_position() {
        let mut history = NpcPresentation::default();
        history.sample(placement(0.0), 0.0, 1.0);
        for fraction in [0.0, 0.5, 1.0] {
            let (rendered, motion) = history.sample(placement(1.0), 1.0 / 60.0, fraction);
            assert!((rendered.position[0] - fraction).abs() < 1e-5);
            assert!(motion.is_some_and(|motion| (motion.speed - 60.0).abs() < 0.001));
        }
        let (_, motion) = history.sample(placement(1.0), 2.0 / 60.0, 1.0);
        assert!(motion.is_some_and(|motion| motion.speed == 0.0));
    }

    #[test]
    fn teleports_and_clock_rewinds_reset_interpolation_without_locomotion() {
        for (x, seconds) in [(1000.0, 2.0), (2.0, 0.0), (2.0, 1.0)] {
            let mut history = NpcPresentation::default();
            history.sample(placement(0.0), 0.0, 1.0);
            history.sample(placement(1.0), 1.0, 1.0);
            let (rendered, motion) = history.sample(placement(x), seconds, 0.0);
            assert_eq!(rendered, placement(x));
            assert!(motion.is_some_and(|motion| motion.speed == 0.0));
        }
    }

    #[test]
    fn heading_interpolation_crosses_the_angle_seam_without_a_full_turn() {
        let mut history = NpcPresentation::default();
        let mut previous = placement(0.0);
        previous.rotation[2] = 3.13;
        let mut current = previous;
        current.rotation[2] = -3.13;
        history.sample(previous, 0.0, 1.0);
        let (rendered, _) = history.sample(current, 1.0 / 60.0, 0.5);
        assert!((rendered.rotation[2].abs() - std::f32::consts::PI).abs() < 0.001);
    }
}
