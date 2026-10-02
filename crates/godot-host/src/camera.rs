// SPDX-License-Identifier: GPL-3.0-or-later

//! Where the camera sits and which way it looks, in Skyrim units and Skyrim
//! conventions, with no engine dependency so the arithmetic is testable
//! headless.
//!
//! Skyrim's yaw is zero facing north (`+Y`) and grows clockwise seen from
//! above, so forward is `(sin yaw, cos yaw, 0)`. Pitch is positive looking
//! up. `units` maps the result into Godot's space, this module never does.
//!
//! ## Where the numbers come from, and where they do not
//!
//! The setting names are vanilla and verified: `fOverShoulderPosX`,
//! `fOverShoulderCombatAddY` and `fOverShoulderPosZ` are the three floats
//! `ThirdPersonState` holds at `0x5C`, `0x60` and `0x64`, which
//! `engine-refs/skse64/skse64/GameCamera.h` declares outright.
//!
//! Their *values* are a different matter. Bethesda compiles the defaults into
//! the executable, the shipped `Skyrim_Default.ini` carries no `[Camera]`
//! section at all, and nothing in `engine-refs/` records them. So the numbers
//! in [`CameraSettings::default`] are this engine's, chosen to sit near the
//! vanilla framing rather than copied from it, and every one of them is
//! overridable from the INI under its vanilla name. A user who wants the
//! exact retail framing writes it into `Skyrim.ini` and gets it, which is the
//! same lever the real game offers. Do not let a later reader mistake these
//! for measured constants.

use soe_esm::GlobalFormId;
use soe_physics::Vec3;

use crate::units::SkyrimPos;

/// Two fixed-step poses for a render-frame camera. Mouse rotation stays live.
/// Follows Godot's advanced physics interpolation guide for manual cameras.
#[derive(Debug, Clone, Copy)]
pub struct CameraMotion {
    previous: (Vec3, f32),
    current: (Vec3, f32),
}

/// Camera controls currently applied by the host player node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraHostState {
    target: Option<GlobalFormId>,
    first_person_geometry_visible: bool,
    sitting_rotation_degrees: f32,
}

impl Default for CameraHostState {
    fn default() -> Self {
        Self {
            target: None,
            first_person_geometry_visible: true,
            sitting_rotation_degrees: 0.0,
        }
    }
}

impl CameraHostState {
    pub fn apply(&mut self, controls: soe_game::world::CameraControls) {
        self.target = controls.target;
        self.first_person_geometry_visible = controls.first_person_geometry;
        self.sitting_rotation_degrees = controls.sitting_rotation_degrees;
    }

    #[must_use]
    pub fn target(&self) -> Option<GlobalFormId> {
        self.target
    }

    #[must_use]
    pub fn first_person_geometry_visible(&self) -> bool {
        self.first_person_geometry_visible
    }

    #[must_use]
    pub fn sitting_rotation_degrees(&self) -> f32 {
        self.sitting_rotation_degrees
    }

    #[must_use]
    pub fn anchor(
        &self,
        player_feet: Vec3,
        player_height: f32,
        target_pose: Option<(GlobalFormId, Vec3, f32)>,
    ) -> (Vec3, f32) {
        match (self.target, target_pose) {
            (Some(target), Some((pose_id, feet, height))) if target == pose_id => (feet, height),
            _ => (player_feet, player_height),
        }
    }
}

impl CameraMotion {
    /// Reset both poses on entry or teleport so the old cell is never blended.
    pub fn new(feet: Vec3, height: f32) -> Self {
        Self {
            previous: (feet, height),
            current: (feet, height),
        }
    }

    pub fn step(&mut self, feet: Vec3, height: f32) {
        self.previous = self.current;
        self.current = (feet, height);
    }

    pub fn sample(&self, fraction: f32) -> (Vec3, f32) {
        let t = fraction.clamp(0.0, 1.0);
        (
            self.previous.0 + (self.current.0 - self.previous.0) * t,
            self.previous.1 + (self.current.1 - self.previous.1) * t,
        )
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;

    #[test]
    fn interpolates_position_and_crouch_without_extrapolation() {
        let mut motion = CameraMotion::new(Vec3::ZERO, 128.0);
        motion.step(Vec3::new(8.0, 4.0, 2.0), 72.0);
        assert_eq!(motion.sample(0.5), (Vec3::new(4.0, 2.0, 1.0), 100.0));
        assert_eq!(motion.sample(-1.0), (Vec3::ZERO, 128.0));
        assert_eq!(motion.sample(2.0), (Vec3::new(8.0, 4.0, 2.0), 72.0));
    }

    #[test]
    fn new_pose_resets_history_and_steps_roll_forward() {
        let feet = Vec3::new(1000.0, 2000.0, 3000.0);
        let mut motion = CameraMotion::new(feet, 128.0);
        assert_eq!(motion.sample(0.0), (feet, 128.0));
        motion.step(feet + Vec3::Z, 128.0);
        motion.step(feet + Vec3::Z * 2.0, 128.0);
        assert_eq!(motion.sample(0.0), (feet + Vec3::Z, 128.0));
    }
}

/// A camera placement in Skyrim space: where the eye is and how it is turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eye {
    pub position: SkyrimPos,
    /// Radians, zero facing north, growing clockwise from above.
    pub yaw: f32,
    /// Radians, positive looking up.
    pub pitch: f32,
}

/// First or third person. Vanity and horse states exist in the real game and
/// are not modelled yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    First,
    Third,
}

impl CameraMode {
    #[must_use]
    pub fn toggled(self) -> Self {
        match self {
            CameraMode::First => CameraMode::Third,
            CameraMode::Third => CameraMode::First,
        }
    }
}

/// Apply one of the explicit Game camera mode requests to the live rig.
pub(crate) fn apply_game_camera_request(rig: &mut CameraRig, first_person: bool) {
    rig.mode = if first_person {
        CameraMode::First
    } else {
        CameraMode::Third
    };
}

/// The tunables, under their vanilla INI names where one exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSettings {
    /// `fOverShoulderPosX`: how far right of the spine the third person
    /// camera sits. Negative puts it over the left shoulder.
    pub over_shoulder_pos_x: f32,
    /// `fOverShoulderPosZ`: vertical nudge from the head, in units.
    pub over_shoulder_pos_z: f32,
    /// `fOverShoulderCombatAddY`: added to the boom length with a weapon out,
    /// negative pulling the camera in.
    pub over_shoulder_combat_add_y: f32,
    /// Resting third person boom length before combat adds to it.
    pub boom_length: f32,
    /// Eye height as a fraction of the capsule's current height, so crouching
    /// lowers the view without a second constant.
    pub eye_height_fraction: f32,
    /// `fMouseHeadingSensitivity`: radians of yaw per unit of mouse motion.
    pub mouse_sensitivity: f32,
    /// How far from straight ahead the pitch may travel, in degrees.
    pub pitch_limit_degrees: f32,
    /// Vertical field of view in degrees, first person and third.
    pub first_person_fov: f32,
    pub world_fov: f32,
    /// How far the boom keeps off geometry it would otherwise clip through.
    pub collision_padding: f32,
}

impl Default for CameraSettings {
    /// This engine's framing, not measured vanilla values. See the module
    /// note before treating any of these as authoritative.
    fn default() -> Self {
        CameraSettings {
            over_shoulder_pos_x: 30.0,
            over_shoulder_pos_z: -10.0,
            over_shoulder_combat_add_y: -50.0,
            boom_length: 150.0,
            eye_height_fraction: 0.9,
            mouse_sensitivity: 0.0025,
            pitch_limit_degrees: 89.0,
            first_person_fov: 75.0,
            world_fov: 65.0,
            collision_padding: 15.0,
        }
    }
}

impl CameraSettings {
    /// Read whatever the layered INI carries under `[Camera]`, leaving every
    /// absent key at its default. The keys are the vanilla names, so an
    /// existing `Skyrim.ini` tuned for the retail game applies unchanged.
    ///
    /// `get` is the caller's lookup rather than a `soe_profile::Settings`
    /// directly, which keeps this crate off the profile crate and lets a test
    /// drive it from a map.
    pub fn apply_ini<F>(&mut self, get: F)
    where
        F: Fn(&str, &str) -> Option<f64>,
    {
        let set = |key: &str, slot: &mut f32| {
            if let Some(v) = get("Camera", key) {
                *slot = v as f32;
            }
        };
        set("fOverShoulderPosX", &mut self.over_shoulder_pos_x);
        set("fOverShoulderPosZ", &mut self.over_shoulder_pos_z);
        set(
            "fOverShoulderCombatAddY",
            &mut self.over_shoulder_combat_add_y,
        );
        set("fMouseHeadingSensitivity", &mut self.mouse_sensitivity);
        set("fDefaultWorldFOV", &mut self.world_fov);
        set("fDefault1stPersonFOV", &mut self.first_person_fov);
    }
}

/// Yaw, pitch and mode, plus the arithmetic that turns them into an eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraRig {
    pub settings: CameraSettings,
    pub mode: CameraMode,
    yaw: f32,
    pitch: f32,
    camera_yaw_offset: f32,
    /// Whether a weapon is drawn, which only changes the boom length.
    pub combat: bool,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self::new(CameraSettings::default())
    }
}

impl CameraRig {
    #[must_use]
    pub fn new(settings: CameraSettings) -> Self {
        CameraRig {
            settings,
            mode: CameraMode::default(),
            yaw: 0.0,
            pitch: 0.0,
            camera_yaw_offset: 0.0,
            combat: false,
        }
    }

    /// Same rig in another mode, so a caller building one in third person
    /// does not have to mutate a default into shape.
    #[must_use]
    pub fn with_mode(mut self, mode: CameraMode) -> Self {
        self.mode = mode;
        self
    }

    /// Same rig with the weapon drawn or sheathed.
    #[must_use]
    pub fn with_combat(mut self, combat: bool) -> Self {
        self.combat = combat;
        self
    }

    #[must_use]
    pub fn yaw(&self) -> f32 {
        self.yaw
    }

    #[must_use]
    pub fn pitch(&self) -> f32 {
        self.pitch
    }

    /// Apply the seated camera's yaw offset without changing movement facing.
    pub fn set_camera_yaw_offset_degrees(&mut self, degrees: f32) {
        self.camera_yaw_offset = degrees.to_radians();
    }

    #[must_use]
    pub fn camera_yaw_offset_degrees(&self) -> f32 {
        self.camera_yaw_offset.to_degrees()
    }

    /// Point the camera outright, used when a save or a door sets a facing.
    pub fn set_facing(&mut self, yaw: f32, pitch: f32) {
        self.yaw = wrap_angle(yaw);
        self.pitch = self.clamp_pitch(pitch);
    }

    /// Apply a frame of mouse motion. `dx` to the right turns right, `dy`
    /// down looks down, which is the uninverted convention.
    pub fn look(&mut self, dx: f32, dy: f32) {
        let s = self.settings.mouse_sensitivity;
        self.yaw = wrap_angle(self.yaw + dx * s);
        self.pitch = self.clamp_pitch(self.pitch - dy * s);
    }

    fn clamp_pitch(&self, pitch: f32) -> f32 {
        let limit = self.settings.pitch_limit_degrees.to_radians();
        pitch.clamp(-limit, limit)
    }

    /// The horizontal direction the camera faces, unit length, Z zero. This
    /// is what the character controller is walked along, so pitch never leaks
    /// into movement.
    #[must_use]
    pub fn forward_flat(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), self.yaw.cos(), 0.0)
    }

    /// The horizontal direction to the camera's right.
    #[must_use]
    pub fn right_flat(&self) -> Vec3 {
        Vec3::new(self.yaw.cos(), -self.yaw.sin(), 0.0)
    }

    /// The full look direction including pitch, for a ray at the crosshair.
    #[must_use]
    pub fn look_direction(&self) -> Vec3 {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        Vec3::new(self.yaw.sin() * cp, self.yaw.cos() * cp, sp)
    }

    /// Turn a movement stick into a world direction. `x` is strafe, positive
    /// right, `y` is forward, positive ahead. The result is flat and
    /// normalised, or zero when the stick is centred, so diagonal movement
    /// does not outrun straight movement.
    #[must_use]
    pub fn move_direction(&self, x: f32, y: f32) -> Vec3 {
        let v = self.right_flat() * x + self.forward_flat() * y;
        let len = (v.x * v.x + v.y * v.y).sqrt();
        if len > 1e-6 {
            Vec3::new(v.x / len, v.y / len, 0.0)
        } else {
            Vec3::new(0.0, 0.0, 0.0)
        }
    }

    /// Where the head is: up the capsule from the feet by the eye fraction.
    #[must_use]
    pub fn head(&self, feet: Vec3, capsule_height: f32) -> Vec3 {
        Vec3::new(
            feet.x,
            feet.y,
            feet.z + capsule_height * self.settings.eye_height_fraction,
        )
    }

    /// The boom length this frame, combat pulling it in.
    #[must_use]
    pub fn boom_length(&self) -> f32 {
        let combat = if self.combat {
            self.settings.over_shoulder_combat_add_y
        } else {
            0.0
        };
        (self.settings.boom_length + combat).max(0.0)
    }

    /// Place the eye. `feet` is the bottom of the capsule and
    /// `capsule_height` its current height, so a crouching character lowers
    /// the view without the caller doing arithmetic.
    ///
    /// `clear_distance` is how far the boom may extend before geometry gets
    /// in the way, normally from a raycast, and `None` means unobstructed. It
    /// is ignored in first person.
    #[must_use]
    pub fn eye(&self, feet: Vec3, capsule_height: f32, clear_distance: Option<f32>) -> Eye {
        let head = self.head(feet, capsule_height);
        let camera_yaw = wrap_angle(self.yaw + self.camera_yaw_offset);
        let camera_look = look_vector(camera_yaw, self.pitch);
        let camera_right = Vec3::new(camera_yaw.cos(), -camera_yaw.sin(), 0.0);
        let position = match self.mode {
            CameraMode::First => head,
            CameraMode::Third => {
                let mut boom = self.boom_length();
                if let Some(clear) = clear_distance {
                    boom = boom.min((clear - self.settings.collision_padding).max(0.0));
                }
                let back = Vec3::new(camera_look.0, camera_look.1, camera_look.2) * -boom;
                let side = camera_right * self.settings.over_shoulder_pos_x;
                Vec3::new(
                    head.x + back.x + side.x,
                    head.y + back.y + side.y,
                    head.z + back.z + side.z + self.settings.over_shoulder_pos_z,
                )
            }
        };
        Eye {
            position: SkyrimPos::new(position.x, position.y, position.z),
            yaw: camera_yaw,
            pitch: self.pitch,
        }
    }

    /// Where a third person boom would reach with nothing in the way, which
    /// is the far end of the ray the host casts to find `clear_distance`.
    #[must_use]
    pub fn boom_target(&self, feet: Vec3, capsule_height: f32) -> Vec3 {
        let head = self.head(feet, capsule_height);
        let yaw = wrap_angle(self.yaw + self.camera_yaw_offset);
        let look = look_vector(yaw, self.pitch);
        let back = Vec3::new(look.0, look.1, look.2) * -self.boom_length();
        Vec3::new(head.x + back.x, head.y + back.y, head.z + back.z)
    }

    /// The field of view that applies in the current mode.
    #[must_use]
    pub fn fov(&self) -> f32 {
        match self.mode {
            CameraMode::First => self.settings.first_person_fov,
            CameraMode::Third => self.settings.world_fov,
        }
    }
}

impl Eye {
    /// The camera's basis in Godot space, as the three columns Godot wants:
    /// right, up, and back.
    ///
    /// Built from explicit columns rather than from Euler angles, because the
    /// order and sign conventions between Skyrim's yaw and Godot's rotations
    /// are exactly the kind of thing that compiles either way and renders
    /// wrong. A column basis can be checked against a known facing, and the
    /// tests do.
    ///
    /// Godot's camera looks down its own `-Z`, so the back column is the
    /// negated look direction. The pitch clamp keeps the look direction off
    /// the world up axis, so the cross products never degenerate.
    #[must_use]
    pub fn basis_columns(&self) -> [[f32; 3]; 3] {
        let look = look_vector(self.yaw, self.pitch);
        let back = normalise(crate::units::direction_to_godot([
            -look.0, -look.1, -look.2,
        ]));
        let world_up = [0.0, 1.0, 0.0];
        let right = normalise(cross(world_up, back));
        let up = cross(back, right);
        [right, up, back]
    }
}

fn look_vector(yaw: f32, pitch: f32) -> (f32, f32, f32) {
    let (cp, sp) = (pitch.cos(), pitch.sin());
    (yaw.sin() * cp, yaw.cos() * cp, sp)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-6 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        v
    }
}

/// Fold an angle into `-pi..pi`, so yaw never grows without bound across a
/// long session and two rigs compare equal at the same facing.
#[must_use]
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let mut a = a % TAU;
    if a > PI {
        a -= TAU;
    } else if a < -PI {
        a += TAU;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHT: f32 = 128.0;

    fn feet() -> Vec3 {
        Vec3::new(0.0, 0.0, 0.0)
    }

    #[test]
    fn yaw_zero_faces_north() {
        let rig = CameraRig::default();
        let f = rig.forward_flat();
        assert!(f.x.abs() < 1e-6, "{}", f.x);
        assert!((f.y - 1.0).abs() < 1e-6, "{}", f.y);
        assert!(f.z.abs() < 1e-6);
    }

    #[test]
    fn yaw_grows_clockwise_so_ninety_degrees_faces_east() {
        let mut rig = CameraRig::default();
        rig.set_facing(std::f32::consts::FRAC_PI_2, 0.0);
        let f = rig.forward_flat();
        assert!((f.x - 1.0).abs() < 1e-6, "{}", f.x);
        assert!(f.y.abs() < 1e-6, "{}", f.y);
    }

    #[test]
    fn right_is_ninety_degrees_clockwise_of_forward() {
        for deg in [0.0_f32, 37.0, 90.0, 180.0, 271.0] {
            let mut rig = CameraRig::default();
            rig.set_facing(deg.to_radians(), 0.0);
            let (f, r) = (rig.forward_flat(), rig.right_flat());
            // Cross of forward and right points down, the clockwise sense.
            let cross_z = f.x * r.y - f.y * r.x;
            assert!((cross_z + 1.0).abs() < 1e-5, "{deg} gave {cross_z}");
        }
    }

    #[test]
    fn pitch_is_clamped_both_ways() {
        let mut rig = CameraRig::default();
        let limit = rig.settings.pitch_limit_degrees.to_radians();
        rig.look(0.0, -10_000.0);
        assert!((rig.pitch() - limit).abs() < 1e-5, "{}", rig.pitch());
        rig.look(0.0, 20_000.0);
        assert!((rig.pitch() + limit).abs() < 1e-5, "{}", rig.pitch());
    }

    #[test]
    fn pitch_never_reaches_straight_up_so_the_basis_stays_defined() {
        let rig = CameraRig::default();
        let limit = rig.settings.pitch_limit_degrees.to_radians();
        assert!(limit < std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn yaw_wraps_instead_of_growing() {
        let mut rig = CameraRig::default();
        for _ in 0..1000 {
            rig.look(100.0, 0.0);
        }
        assert!(
            rig.yaw().abs() <= std::f32::consts::PI + 1e-6,
            "{}",
            rig.yaw()
        );
    }

    #[test]
    fn mouse_right_turns_right() {
        let mut rig = CameraRig::default();
        rig.look(100.0, 0.0);
        // Turning right from north swings forward toward east, positive X.
        assert!(rig.forward_flat().x > 0.0);
    }

    #[test]
    fn mouse_down_looks_down() {
        let mut rig = CameraRig::default();
        rig.look(0.0, 100.0);
        assert!(rig.pitch() < 0.0, "{}", rig.pitch());
        assert!(rig.look_direction().z < 0.0);
    }

    #[test]
    fn diagonal_movement_does_not_outrun_straight() {
        let rig = CameraRig::default();
        let straight = rig.move_direction(0.0, 1.0);
        let diagonal = rig.move_direction(1.0, 1.0);
        let len = |v: Vec3| (v.x * v.x + v.y * v.y).sqrt();
        assert!((len(straight) - 1.0).abs() < 1e-6);
        assert!((len(diagonal) - 1.0).abs() < 1e-6, "{}", len(diagonal));
    }

    #[test]
    fn a_centred_stick_asks_for_no_movement() {
        let rig = CameraRig::default();
        let v = rig.move_direction(0.0, 0.0);
        assert_eq!((v.x, v.y, v.z), (0.0, 0.0, 0.0));
    }

    #[test]
    fn movement_is_relative_to_where_the_camera_looks() {
        let mut rig = CameraRig::default();
        rig.set_facing(std::f32::consts::FRAC_PI_2, 0.0);
        let ahead = rig.move_direction(0.0, 1.0);
        assert!((ahead.x - 1.0).abs() < 1e-6, "facing east, forward is east");
    }

    #[test]
    fn movement_ignores_pitch() {
        let mut rig = CameraRig::default();
        rig.set_facing(0.0, 1.0);
        let v = rig.move_direction(0.0, 1.0);
        assert!(v.z.abs() < 1e-9, "pitch leaked into movement: {}", v.z);
        assert!((v.y - 1.0).abs() < 1e-6);
    }

    #[test]
    fn first_person_eye_sits_in_the_head_not_the_feet() {
        let rig = CameraRig::default();
        let eye = rig.eye(feet(), HEIGHT, None);
        let want = HEIGHT * rig.settings.eye_height_fraction;
        assert!((eye.position.z - want).abs() < 1e-4, "{}", eye.position.z);
        assert!(eye.position.x.abs() < 1e-6 && eye.position.y.abs() < 1e-6);
    }

    #[test]
    fn crouching_lowers_the_eye() {
        let rig = CameraRig::default();
        let standing = rig.eye(feet(), 128.0, None).position.z;
        let crouched = rig.eye(feet(), 72.0, None).position.z;
        assert!(crouched < standing, "{crouched} vs {standing}");
    }

    #[test]
    fn third_person_pulls_back_along_the_look_direction() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let eye = rig.eye(feet(), HEIGHT, None);
        // Facing north, the camera sits south of the head.
        assert!(eye.position.y < 0.0, "{}", eye.position.y);
        let back = (-eye.position.y - rig.boom_length()).abs();
        assert!(back < 1.0, "boom length not honoured, off by {back}");
    }

    #[test]
    fn the_shoulder_offset_moves_the_camera_sideways() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let eye = rig.eye(feet(), HEIGHT, None);
        // Facing north, right is east, so a positive offset adds X.
        assert!(
            (eye.position.x - rig.settings.over_shoulder_pos_x).abs() < 1e-4,
            "{}",
            eye.position.x
        );
    }

    #[test]
    fn combat_pulls_the_boom_in() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let resting = rig.boom_length();
        assert!(rig.with_combat(true).boom_length() < resting);
    }

    #[test]
    fn a_boom_cannot_go_negative_however_far_combat_pulls() {
        let settings = CameraSettings {
            over_shoulder_combat_add_y: -10_000.0,
            ..Default::default()
        };
        let rig = CameraRig::new(settings).with_combat(true);
        assert_eq!(rig.boom_length(), 0.0);
    }

    #[test]
    fn an_obstruction_pulls_the_camera_in_and_keeps_its_padding() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let free = rig.eye(feet(), HEIGHT, None).position;
        let blocked = rig.eye(feet(), HEIGHT, Some(50.0)).position;
        assert!(blocked.y > free.y, "obstruction did not pull the camera in");
        let head_to_eye = (blocked.y - rig.head(feet(), HEIGHT).y).abs();
        assert!(
            head_to_eye <= 50.0 - rig.settings.collision_padding + 1.0,
            "padding not kept, {head_to_eye}"
        );
    }

    #[test]
    fn an_obstruction_inside_the_padding_collapses_to_the_head() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let eye = rig.eye(feet(), HEIGHT, Some(1.0));
        let head = rig.head(feet(), HEIGHT);
        // Only the shoulder offset remains, the boom is fully collapsed.
        assert!((eye.position.y - head.y).abs() < 1e-4, "{}", eye.position.y);
    }

    #[test]
    fn first_person_ignores_an_obstruction() {
        let rig = CameraRig::default();
        let free = rig.eye(feet(), HEIGHT, None);
        let blocked = rig.eye(feet(), HEIGHT, Some(1.0));
        assert_eq!(free, blocked);
    }

    #[test]
    fn the_boom_target_is_where_the_ray_should_end() {
        let rig = CameraRig::default().with_mode(CameraMode::Third);
        let head = rig.head(feet(), HEIGHT);
        let target = rig.boom_target(feet(), HEIGHT);
        let d = ((target.x - head.x).powi(2)
            + (target.y - head.y).powi(2)
            + (target.z - head.z).powi(2))
        .sqrt();
        assert!((d - rig.boom_length()).abs() < 1e-3, "{d}");
    }

    #[test]
    fn toggling_twice_returns_to_where_it_started() {
        let m = CameraMode::First;
        assert_eq!(m.toggled().toggled(), m);
        assert_eq!(m.toggled(), CameraMode::Third);
    }

    #[test]
    fn the_fov_follows_the_mode() {
        let mut rig = CameraRig::default();
        assert_eq!(rig.fov(), rig.settings.first_person_fov);
        rig.mode = CameraMode::Third;
        assert_eq!(rig.fov(), rig.settings.world_fov);
    }

    #[test]
    fn the_ini_overrides_under_vanilla_names() {
        let mut settings = CameraSettings::default();
        settings.apply_ini(|section, key| match (section, key) {
            ("Camera", "fOverShoulderPosX") => Some(-30.0),
            ("Camera", "fDefaultWorldFOV") => Some(90.0),
            _ => None,
        });
        assert_eq!(settings.over_shoulder_pos_x, -30.0);
        assert_eq!(settings.world_fov, 90.0);
        // Everything absent keeps its default.
        assert_eq!(
            settings.over_shoulder_pos_z,
            CameraSettings::default().over_shoulder_pos_z
        );
    }

    #[test]
    fn an_empty_ini_changes_nothing() {
        let mut settings = CameraSettings::default();
        settings.apply_ini(|_, _| None);
        assert_eq!(settings, CameraSettings::default());
    }

    #[test]
    fn a_left_shoulder_offset_mirrors_the_camera() {
        let settings = CameraSettings {
            over_shoulder_pos_x: -30.0,
            ..Default::default()
        };
        let rig = CameraRig::new(settings).with_mode(CameraMode::Third);
        let eye = rig.eye(feet(), HEIGHT, None);
        assert!(eye.position.x < 0.0, "{}", eye.position.x);
    }

    fn close(a: [f32; 3], b: [f32; 3], what: &str) {
        for i in 0..3 {
            assert!(
                (a[i] - b[i]).abs() < 1e-5,
                "{what}: {a:?} is not {b:?} at {i}"
            );
        }
    }

    #[test]
    fn facing_north_looks_down_godot_negative_z() {
        // Skyrim north is Godot -Z, and a Godot camera looks down its own -Z,
        // so the back column is +Z and the basis is the identity.
        let rig = CameraRig::default();
        let [right, up, back] = rig.eye(feet(), HEIGHT, None).basis_columns();
        close(right, [1.0, 0.0, 0.0], "right");
        close(up, [0.0, 1.0, 0.0], "up");
        close(back, [0.0, 0.0, 1.0], "back");
    }

    #[test]
    fn facing_east_turns_the_basis_a_quarter_turn() {
        let mut rig = CameraRig::default();
        rig.set_facing(std::f32::consts::FRAC_PI_2, 0.0);
        let [right, up, back] = rig.eye(feet(), HEIGHT, None).basis_columns();
        // East is Godot +X, so the camera looks along +X and its back is -X.
        close(back, [-1.0, 0.0, 0.0], "back");
        close(right, [0.0, 0.0, 1.0], "right");
        close(up, [0.0, 1.0, 0.0], "up");
    }

    #[test]
    fn looking_up_tilts_the_back_column_down() {
        let mut rig = CameraRig::default();
        rig.set_facing(0.0, 0.5);
        let [_, up, back] = rig.eye(feet(), HEIGHT, None).basis_columns();
        // Looking up means the camera's back points down and behind.
        assert!(back[1] < 0.0, "back did not tilt down: {back:?}");
        assert!(back[2] > 0.0, "back stopped pointing behind: {back:?}");
        assert!(up[1] > 0.0, "up flipped: {up:?}");
    }

    #[test]
    fn the_basis_stays_orthonormal_across_the_whole_range() {
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        for yaw_deg in [-180.0_f32, -91.0, 0.0, 47.0, 179.0] {
            for pitch_deg in [-89.0_f32, -45.0, 0.0, 45.0, 89.0] {
                let mut rig = CameraRig::default();
                rig.set_facing(yaw_deg.to_radians(), pitch_deg.to_radians());
                let [x, y, z] = rig.eye(feet(), HEIGHT, None).basis_columns();
                let what = format!("yaw {yaw_deg} pitch {pitch_deg}");
                for (v, name) in [(x, "x"), (y, "y"), (z, "z")] {
                    assert!((dot(v, v) - 1.0).abs() < 1e-4, "{what}: {name} not unit");
                }
                assert!(dot(x, y).abs() < 1e-4, "{what}: x and y not perpendicular");
                assert!(dot(y, z).abs() < 1e-4, "{what}: y and z not perpendicular");
                assert!(dot(x, z).abs() < 1e-4, "{what}: x and z not perpendicular");
                // Right-handed: x cross y must be z, not -z.
                close(cross(x, y), z, &format!("{what}: handedness"));
            }
        }
    }

    #[test]
    fn the_back_column_is_the_negated_look_direction() {
        let mut rig = CameraRig::default();
        rig.set_facing(1.1, -0.4);
        let eye = rig.eye(feet(), HEIGHT, None);
        let look = rig.look_direction();
        let want = crate::units::direction_to_godot([-look.x, -look.y, -look.z]);
        close(eye.basis_columns()[2], normalise(want), "back");
    }

    #[test]
    fn wrap_angle_folds_into_one_turn() {
        use std::f32::consts::{PI, TAU};
        assert!((wrap_angle(0.0)).abs() < 1e-6);
        assert!((wrap_angle(TAU) - 0.0).abs() < 1e-5);
        assert!(wrap_angle(3.0 * PI) <= PI + 1e-5);
        assert!(wrap_angle(-3.0 * PI) >= -PI - 1e-5);
    }

    #[test]
    fn scr_28_game_camera_requests_change_the_live_rig_mode() {
        let mut rig = CameraRig::default().with_mode(CameraMode::Third);
        apply_game_camera_request(&mut rig, true);
        assert_eq!(rig.mode, CameraMode::First);
        apply_game_camera_request(&mut rig, false);
        assert_eq!(rig.mode, CameraMode::Third);
    }
}
