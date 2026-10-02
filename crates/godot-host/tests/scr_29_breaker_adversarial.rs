// SPDX-License-Identifier: GPL-3.0-or-later

//! SCR-29 presentation interactions beyond the basic host-state checks.

use soe_godot_host::hud::GameUiPresentation;

#[test]
fn scr_29_cart_mode_repeated_assignments_keep_the_last_authored_flag() {
    let mut presentation = GameUiPresentation::default();
    for enabled in [true, true, false, false, true] {
        presentation.set_cart_mode(enabled);
    }
    assert!(presentation.cart_mode);
    presentation.set_cart_mode(false);
    assert!(!presentation.cart_mode);
}

#[test]
fn scr_29_screen_blood_does_not_rewrite_the_cart_mode_flag() {
    let mut presentation = GameUiPresentation::default();
    presentation.set_cart_mode(true);
    presentation.trigger_screen_blood(50);
    assert!(presentation.cart_mode);
}
