// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_game::world::HostRequest;
use soe_godot_host::ui::{apply_host_request, MainMenuState};

#[test]
fn ui_18_breaker2_sequence_request_enters_menu_and_quit_clears_it() {
    let mut menu = MainMenuState::default();
    assert!(!menu.is_active());
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("MQ101Intro".into()),
    );
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), Some("MQ101Intro"));
    apply_host_request(&mut menu, HostRequest::QuitToMainMenu);
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}

#[test]
fn ui_18_breaker2_leaving_main_menu_clears_the_sequence_for_new_game() {
    let mut menu = MainMenuState::default();
    apply_host_request(
        &mut menu,
        HostRequest::StartTitleSequence("MQ101Intro".into()),
    );
    menu.leave();
    assert!(!menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}
