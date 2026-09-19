// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::camera::CameraRig;
use crate::input::{ControlMap, Driver, InputState};
use soe_physics::JOG_SPEED;

fn bindings(text: &str) -> Bindings {
    let mut result = Bindings::default();
    result.apply_control_map(&ControlMap::parse(text));
    result
}

#[test]
fn a_chord_needs_one_pad_and_short_taps_survive_until_consumed() {
    let b = bindings("Toggle POV\t0xff\t0xff\t0x100+0x200,0x1000\t1\t1\t1");
    let mut d = Devices::default();
    d.pad_button(0, 9, true, &b);
    d.pad_button(1, 10, true, &b);
    assert!(!d.held(Action::TogglePov));
    assert!(d.take_pressed().is_empty());
    d.pad_button(0, 10, true, &b);
    d.pad_button(0, 10, false, &b);
    assert!(!d.held(Action::TogglePov));
    assert!(d.take_pressed().contains(&Action::TogglePov));
    assert!(d.take_pressed().is_empty());
    d.pad_button(1, 0, true, &b);
    assert!(d.held(Action::TogglePov));
    assert!(d.take_pressed().contains(&Action::TogglePov));
}

#[test]
fn mouse_chords_wheel_pulses_and_unavailable_buttons() {
    let b = bindings("Toggle POV\t0xff\t0x0+0x1,0x8,0x5\t0xff\t1\t1\t1");
    let mut d = Devices::default();
    d.mouse_button(2, true, &b);
    assert!(d.take_pressed().is_empty());
    d.mouse_button(1, true, &b);
    assert!(d.take_pressed().contains(&Action::TogglePov));
    d.mouse_button(1, false, &b);
    d.mouse_button(2, false, &b);
    for _ in 0..2 {
        d.mouse_button(4, true, &b);
        assert!(d.take_pressed().contains(&Action::TogglePov));
        assert!(!d.held(Action::TogglePov));
    }
    assert_eq!(godot_mouse_for(5), None);
    assert_eq!(godot_mouse_for(3), Some(8));
    assert_eq!(godot_mouse_for(9), Some(5));
}

#[test]
fn trigger_threshold_and_rebinding_do_not_create_extra_edges() {
    let b = bindings("Sneak\t0xff\t0xff\t0x9\t1\t1\t1");
    let mut d = Devices::default();
    d.pad_axis(0, 4, TRIGGER_THRESHOLD, &b);
    assert!(!d.held(Action::Sneak));
    d.pad_axis(0, 4, 0.8, &b);
    assert!(d.take_pressed().contains(&Action::Sneak));
    d.pad_axis(0, 4, 1.0, &b);
    assert!(d.take_pressed().is_empty());
    let new = bindings("Toggle POV\t0xff\t0xff\t0x9\t1\t1\t1");
    d.rebind(&new);
    assert!(!d.held(Action::Sneak));
    assert!(d.held(Action::TogglePov));
    assert!(d.take_pressed().is_empty());
    d.pad_axis(0, 4, 0.0, &new);
    d.pad_axis(0, 4, 0.9, &new);
    assert!(d.take_pressed().contains(&Action::TogglePov));
    d.rebind(&Bindings::default());
    assert!(!d.held(Action::TogglePov));
    assert!(d.take_pressed().is_empty());
}

#[test]
fn stick_dead_zone_scaling_disconnect_and_bad_axes() {
    let b = bindings("Move\t0xff\t0xff\t0xb\t0\t0\t0\nLook\t0xff\t0xff\t0xc\t0\t0\t0");
    let mut d = Devices::default();
    d.pad_axis(0, 0, 0.1, &b);
    d.pad_axis(0, 1, -0.1, &b);
    assert_eq!(d.movement(&b), (0.0, -0.0));
    d.pad_axis(0, 0, 0.0, &b);
    d.pad_axis(0, 1, -0.6, &b);
    assert!((d.movement(&b).1 - 0.5).abs() < 1e-6);
    d.pad_axis(1, 0, 1.0, &b);
    d.pad_axis(1, 1, 1.0, &b);
    let (x, y) = d.movement(&b);
    assert!((x.hypot(y) - 1.0).abs() < 1e-6);
    d.disconnect(1, &b);
    assert!((d.movement(&b).1 - 0.5).abs() < 1e-6);
    d.pad_axis(0, 2, f32::NAN, &b);
    d.pad_axis(0, 3, f32::INFINITY, &b);
    d.pad_axis(0, -1, 1.0, &b);
    assert_eq!(d.look(&b), (0.0, 0.0));
    d.disconnect(0, &b);
    assert_eq!(d.movement(&b), (0.0, -0.0));
}

#[test]
fn remapped_stick_chords_and_analog_speed() {
    let b = bindings("Move\t0xff\t0xff\t0x100+0xc\t0\t0\t0");
    let mut d = Devices::default();
    d.pad_axis(0, 3, -0.6, &b);
    d.pad_button(1, 9, true, &b);
    assert_eq!(d.movement(&b), (0.0, -0.0));
    d.pad_button(0, 9, true, &b);
    let (move_x, move_y) = d.movement(&b);
    let mut driver = Driver::new();
    let intent = driver.update(
        &InputState {
            move_x,
            move_y,
            ..InputState::default()
        },
        &CameraRig::default(),
    );
    assert!((intent.speed - JOG_SPEED * 0.5).abs() < 1e-4);
    let diagonal = driver.update(
        &InputState {
            move_x: 1.0,
            move_y: 1.0,
            ..InputState::default()
        },
        &CameraRig::default(),
    );
    assert_eq!(diagonal.speed, JOG_SPEED);
}

#[test]
fn mixed_device_edges_and_disconnect_preserve_received_presses() {
    let b = bindings("Toggle POV\t0x21\t0x8\t0x1000\t1\t1\t1");
    let mut d = Devices::default();
    d.key(i64::from(b'F'), true, &b);
    assert!(d.take_pressed().contains(&Action::TogglePov));
    d.pad_button(0, 0, true, &b);
    assert!(
        d.take_pressed().is_empty(),
        "another device cannot repress a held action"
    );
    d.key(i64::from(b'F'), false, &b);
    d.pad_button(0, 0, false, &b);
    d.key(i64::from(b'F'), true, &b);
    d.key(i64::from(b'F'), false, &b);
    assert!(d.take_pressed().contains(&Action::TogglePov));
    d.mouse_button(4, true, &b);
    d.disconnect(7, &b);
    assert!(d.take_pressed().contains(&Action::TogglePov));
    d.pad_button(1, 0, true, &b);
    d.pad_button(1, 0, false, &b);
    d.disconnect(0, &b);
    assert!(d.take_pressed().contains(&Action::TogglePov));
}
