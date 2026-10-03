// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Engine behavior around the existing native menu shell.

use soe_godot_host::ui::MenuState;

#[test]
fn rejected_blank_titles_do_not_create_or_replace_a_menu() {
    let mut menu = MenuState::default();
    for title in ["", "   ", "\t\n"] {
        assert!(!menu.open(title));
        assert!(!menu.is_open());
        assert_eq!(menu.title(), None);
    }

    assert!(menu.open("Inventory"));
    assert!(!menu.open(" \t "));
    assert_eq!(menu.title(), Some("Inventory"));
}

#[test]
fn second_open_keeps_the_first_native_menu_until_closed() {
    let mut menu = MenuState::default();
    assert!(menu.open("Map"));
    assert!(!menu.open("Dialogue"));
    assert_eq!(menu.title(), Some("Map"));
    assert!(menu.close());
    assert!(menu.open("Dialogue"));
    assert_eq!(menu.title(), Some("Dialogue"));
}

#[test]
fn close_is_idempotent_and_clears_the_current_title() {
    let mut menu = MenuState::default();
    assert!(!menu.close());
    assert!(menu.open("Container"));
    assert!(menu.close());
    assert!(!menu.is_open());
    assert_eq!(menu.title(), None);
    assert!(!menu.close());
}

#[test]
fn repeated_open_close_cycles_do_not_retain_stale_titles() {
    let mut menu = MenuState::default();
    for title in ["Inventory", "魔法", "A very long menu title".repeat(32).as_str()] {
        assert!(menu.open(title));
        assert_eq!(menu.title(), Some(title));
        assert!(menu.close());
        assert_eq!(menu.title(), None);
    }
}
