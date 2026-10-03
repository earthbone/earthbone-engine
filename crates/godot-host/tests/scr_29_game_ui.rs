// SPDX-License-Identifier: GPL-3.0-or-later

//! SCR-29 host presentation state for the cart HUD and screen blood effect.

use soe_godot_host::hud::GameUiPresentation;

#[test]
fn scr_29_cart_mode_is_reflected_in_host_presentation_state() {
    let mut presentation = GameUiPresentation::default();
    assert!(!presentation.cart_mode);
    presentation.set_cart_mode(true);
    assert!(presentation.cart_mode);
    presentation.set_cart_mode(false);
    assert!(!presentation.cart_mode);
}

#[test]
fn scr_29_screen_blood_maps_intensity_and_expires() {
    let mut presentation = GameUiPresentation::default();
    presentation.trigger_screen_blood(-75);
    assert_eq!(presentation.blood_alpha, 0.75);
    presentation.tick(0.2);
    assert_eq!(presentation.blood_alpha, 0.75);
    presentation.tick(0.25);
    assert_eq!(presentation.blood_alpha, 0.0);
}

#[test]
fn scr_29_screen_blood_bounds_intensity_and_zero_clears() {
    let mut presentation = GameUiPresentation::default();
    presentation.trigger_screen_blood(i32::MIN);
    assert_eq!(presentation.blood_alpha, 1.0);
    presentation.trigger_screen_blood(0);
    assert_eq!(presentation.blood_alpha, 0.0);
}
