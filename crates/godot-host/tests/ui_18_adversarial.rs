// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_game::world::HostRequest;
use soe_godot_host::ui::{apply_host_request, MainMenuState};

#[test]
fn ui_18_adversarial_quit_clears_a_sequence_and_repeated_quit_stays_at_menu() {
    let mut menu = MainMenuState::default();
    menu.start_sequence("Intro".into());
    apply_host_request(&mut menu, HostRequest::QuitToMainMenu);
    apply_host_request(&mut menu, HostRequest::QuitToMainMenu);
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), None);
}

#[test]
fn ui_18_adversarial_later_title_request_replaces_the_current_sequence() {
    let mut menu = MainMenuState::default();
    apply_host_request(&mut menu, HostRequest::StartTitleSequence("First".into()));
    apply_host_request(&mut menu, HostRequest::StartTitleSequence("Second".into()));
    assert!(menu.is_active());
    assert_eq!(menu.title_sequence(), Some("Second"));
}
