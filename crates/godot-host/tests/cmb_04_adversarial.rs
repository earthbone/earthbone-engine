// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

//! CMB-04 attack input edge checks.
//!
//! The row's check is one injected attack press producing one shared Game hit.
//! The local PC control map names the event `Right Attack/Block`; device edges
//! come from `input::devices::Devices::observe` and are consumed once.

use soe_godot_host::input::devices::Devices;
use soe_godot_host::input::{Action, Bindings, ControlMap, InputState};

#[test]
fn cmb_04_keyboard_attack_fires_once_per_press_not_once_per_held_frame() {
    let mut bindings = Bindings::default();
    bindings.apply_control_map(&ControlMap::parse(
        "Right Attack/Block\t0x2e\t0xff\t0xff\t1\t1\t1\n",
    ));
    let down = |key| key == 0x2e;
    let held_before = |key| key == 0x2e;

    assert!(InputState::gather(&bindings, &down, &|_| false, 0.0, 0.0).right_attack_pressed);
    assert!(!InputState::gather(&bindings, &down, &held_before, 0.0, 0.0).right_attack_pressed);
    assert!(InputState::gather(&bindings, &down, &|_| false, 0.0, 0.0).right_attack_pressed);
    assert_eq!(Action::RightAttack.event_name(), "Right Attack/Block");
}

#[test]
fn cmb_04_mouse_attack_repeated_down_does_not_queue_duplicate_presses() {
    let mut bindings = Bindings::default();
    bindings.apply_control_map(&ControlMap::parse(
        "Right Attack/Block\t0xff\t0x1\t0xff\t1\t1\t1\n",
    ));
    let mut devices = Devices::default();

    devices.mouse_button(2, true, &bindings);
    assert!(devices.take_pressed().contains(&Action::RightAttack));
    devices.mouse_button(2, true, &bindings);
    assert!(!devices.take_pressed().contains(&Action::RightAttack));
    devices.mouse_button(2, false, &bindings);
    devices.mouse_button(2, true, &bindings);
    assert!(devices.take_pressed().contains(&Action::RightAttack));
}

#[test]
fn cmb_04_simultaneous_device_bindings_still_make_one_action_edge() {
    let mut bindings = Bindings::default();
    bindings.apply_control_map(&ControlMap::parse(
        "Right Attack/Block\t0x2e\t0x1\t0x1000\t1\t1\t1\n",
    ));
    let mut devices = Devices::default();

    devices.key(67, true, &bindings);
    devices.mouse_button(2, true, &bindings);
    devices.pad_button(0, 0, true, &bindings);
    let presses = devices.take_pressed();
    assert_eq!(
        presses
            .iter()
            .filter(|&&action| action == Action::RightAttack)
            .count(),
        1
    );
    assert!(devices.take_pressed().is_empty());
}
