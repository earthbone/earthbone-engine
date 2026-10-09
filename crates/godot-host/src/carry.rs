// SPDX-License-Identifier: GPL-3.0-or-later

//! Held Activate and a damped carry spring. Sources: CK OnGrab/OnRelease,
//! nif.xml hkMotionType and Godot PhysicsServer3D impulse semantics.
//! The spring uses the resident Jolt body, retaining collision and velocity.

use soe_esm::GlobalFormId;
use soe_physics::Vec3;

pub(crate) const HOLD_SECONDS: f64 = 0.35;

#[derive(Default)]
pub(crate) struct ActivateHold {
    pending: Option<(Option<GlobalFormId>, f64, bool)>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ActivateGesture {
    None,
    Tap(GlobalFormId),
    Grab(GlobalFormId),
}

impl ActivateHold {
    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }

    pub(crate) fn step(
        &mut self,
        pressed: bool,
        held: bool,
        delta: f64,
        target: Option<GlobalFormId>,
    ) -> ActivateGesture {
        if pressed {
            self.pending = Some((target, 0.0, false));
        }
        let Some((candidate, elapsed, consumed)) = &mut self.pending else {
            return ActivateGesture::None;
        };
        if !held {
            let tap = (!*consumed).then_some(*candidate).flatten();
            self.pending = None;
            return tap.map_or(ActivateGesture::None, ActivateGesture::Tap);
        }
        if delta.is_finite() && delta > 0.0 {
            *elapsed += delta;
        }
        if !*consumed && *elapsed >= HOLD_SECONDS {
            *consumed = true;
            if *candidate == target {
                return target.map_or(ActivateGesture::None, ActivateGesture::Grab);
            }
        }
        ActivateGesture::None
    }
}

pub(crate) fn spring_impulse(
    error: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    mass: f32,
    delta: f32,
) -> Option<Vec3> {
    if ![error, velocity, gravity]
        .iter()
        .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite())
        || !mass.is_finite()
        || mass <= 0.0
        || !delta.is_finite()
        || delta <= 0.0
    {
        return None;
    }
    let acceleration = error * 100.0 - velocity * 20.0 - gravity;
    let length = acceleration.length();
    if !length.is_finite() {
        return None;
    }
    let acceleration = if length > 80.0 {
        acceleration * (80.0 / length)
    } else {
        acceleration
    };
    let impulse = acceleration * (mass * delta.min(1.0 / 30.0));
    (impulse.x.is_finite() && impulse.y.is_finite() && impulse.z.is_finite()).then_some(impulse)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: GlobalFormId = GlobalFormId {
        file: 0,
        object: 42,
    };

    #[test]
    fn hst_22_tap_activates_once_on_release_including_between_steps() {
        let mut hold = ActivateHold::default();
        assert_eq!(
            hold.step(true, true, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.1, None),
            ActivateGesture::Tap(TARGET)
        );
        assert_eq!(hold.step(false, false, 0.1, None), ActivateGesture::None);
        assert_eq!(
            hold.step(true, false, 0.01, Some(TARGET)),
            ActivateGesture::Tap(TARGET)
        );
    }

    #[test]
    fn hst_22_hold_grabs_once_and_release_does_not_activate() {
        let mut hold = ActivateHold::default();
        assert_eq!(
            hold.step(true, true, HOLD_SECONDS - 0.01, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, true, 0.02, Some(TARGET)),
            ActivateGesture::Grab(TARGET)
        );
        assert_eq!(
            hold.step(false, true, 1.0, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.01, Some(TARGET)),
            ActivateGesture::None
        );
    }

    #[test]
    fn hst_22_cancel_or_changed_aim_does_not_activate_or_grab() {
        let mut hold = ActivateHold::default();
        hold.step(true, true, 0.1, Some(TARGET));
        assert_eq!(
            hold.step(false, true, HOLD_SECONDS, None),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
        hold.step(true, true, 0.1, Some(TARGET));
        hold.cancel();
        assert_eq!(
            hold.step(false, false, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
    }

    #[test]
    fn hst_22_tap_uses_fixed_container_without_grabbing() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        plugin.push_top(soe_esm::Record::new(soe_esm::Tag::new(b"CONT"), 41));
        plugin.push_top(
            soe_esm::Record::new(soe_esm::tag::REFR, TARGET.object)
                .sub(b"NAME", 41_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let mut game = soe_game::Game::new(order);
        let player = game.player();
        let mut hold = ActivateHold::default();
        hold.step(true, true, 0.1, Some(TARGET));
        let ActivateGesture::Tap(reference) = hold.step(false, false, 0.01, Some(TARGET)) else {
            panic!("short Activate must use the container");
        };
        assert!(game.activate_deferred(reference, player));
        assert!(game.world().reference_ref(reference).unwrap().opened);
        assert_eq!(game.world().grabbed_reference(), None);
    }

    #[test]
    fn hst_22_spring_pulls_damps_and_compensates_gravity() {
        let impulse = spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * 0.1,
            Vec3::new(1.0, 0.0, 0.0) * 0.2,
            -Vec3::Z * 10.0,
            2.0,
            0.01,
        )
        .unwrap();
        assert!((impulse.x - 0.12).abs() < 1e-6);
        assert!((impulse.z - 0.2).abs() < 1e-6);
        let braking =
            spring_impulse(Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO, 1.0, 0.01).unwrap();
        assert!(braking.x < 0.0);
    }

    #[test]
    fn hst_22_spring_bounds_force_and_rejects_invalid_inputs() {
        let impulse = spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * 100.0,
            Vec3::ZERO,
            Vec3::ZERO,
            2.0,
            1.0,
        )
        .unwrap();
        assert!((impulse.length() - 160.0 / 30.0).abs() < 1e-5);
        for mass in [0.0, -1.0, f32::NAN] {
            assert!(spring_impulse(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, mass, 0.01).is_none());
        }
        assert!(spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * f32::NAN,
            Vec3::ZERO,
            Vec3::ZERO,
            1.0,
            0.01
        )
        .is_none());
        assert!(spring_impulse(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 1.0, f32::INFINITY).is_none());
    }
}
