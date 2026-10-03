// SPDX-License-Identifier: GPL-3.0-or-later

//! Additional SCR-29 host presentation edge checks.

use soe_godot_host::hud::GameUiPresentation;

#[test]
fn scr_29_new_blood_request_restarts_overlay_duration_and_zero_clears() {
    let mut presentation = GameUiPresentation::default();
    presentation.trigger_screen_blood(60);
    presentation.tick(0.4);
    assert!(presentation.blood_alpha > 0.0);
    presentation.trigger_screen_blood(25);
    assert_eq!(presentation.blood_alpha, 0.25);
    presentation.tick(0.4);
    assert_eq!(presentation.blood_alpha, 0.25);
    presentation.trigger_screen_blood(0);
    presentation.tick(0.0);
    assert_eq!(presentation.blood_alpha, 0.0);
}

#[test]
fn scr_29_negative_tick_does_not_extend_or_increase_blood_overlay() {
    let mut presentation = GameUiPresentation::default();
    presentation.trigger_screen_blood(40);
    presentation.tick(-1.0);
    assert_eq!(presentation.blood_alpha, 0.4);
    presentation.tick(0.45);
    assert_eq!(presentation.blood_alpha, 0.0);
}
