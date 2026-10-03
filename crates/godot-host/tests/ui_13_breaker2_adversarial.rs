// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::input::{Bindings, ControlMap};
use soe_godot_host::mcm_menu::KeyConflictScanner;

#[test]
fn ui_13_live_conflict_scan_finds_non_gameplay_mouse_controls() {
    let mut bindings = Bindings::vanilla_keyboard();
    bindings.apply_control_map(&ControlMap::parse("Journal\t0xff\t0x23\t0xff\t1\t1\t1\n"));

    assert_eq!(
        KeyConflictScanner::from_bindings(&bindings)
            .conflict(0x23)
            .control,
        "Journal"
    );
}

#[test]
fn ui_13_live_conflict_scan_finds_non_gameplay_gamepad_controls() {
    let mut bindings = Bindings::vanilla_keyboard();
    bindings.apply_control_map(&ControlMap::parse("Favorites\t0xff\t0xff\t0x31\t1\t1\t1\n"));

    assert_eq!(
        KeyConflictScanner::from_bindings(&bindings)
            .conflict(0x31)
            .control,
        "Favorites"
    );
}
