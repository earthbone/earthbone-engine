// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_game::world::HostRequest;
use soe_godot_host::ui::{apply_host_request, MainMenuState};

#[test]
fn ui_18_breaker_starting_game_clears_the_previous_title_sequence() {
    let mut menu = MainMenuState::default();
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("MQ101Intro".into()),
    );

    menu.leave();

    assert!(!menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}

#[test]
fn ui_18_breaker_title_sequence_after_quit_starts_from_a_clean_menu() {
    let mut menu = MainMenuState::default();
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("OldSequence".into()),
    );
    apply_host_request(&mut menu, HostRequest::QuitToMainMenu);
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("MQ101Intro".into()),
    );

    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), Some("MQ101Intro"));
}

#[test]
fn ui_18_breaker_unrelated_host_requests_preserve_the_active_title_sequence() {
    let mut menu = MainMenuState::default();
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("MQ101Intro".into()),
    );
    apply_host_request(
        &mut menu,
        HostRequest::FadeOutGame {
            fading_out: true,
            black: true,
            seconds_before_fade: 0.0,
            fade_duration: 1.0,
        },
    );

    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), Some("MQ101Intro"));
}
