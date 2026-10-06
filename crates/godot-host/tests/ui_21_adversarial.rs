// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::ui::MenuState;

#[test]
fn menu_state_rejects_empty_section_names_and_preserves_existing_state() {
    let mut menu = MenuState::default();
    assert!(menu.open("SKSE Menu Framework"));
    for title in ["", " \t\n"] {
        assert!(!menu.open(title));
        assert_eq!(menu.title(), Some("SKSE Menu Framework"));
    }
    assert!(menu.close());
    assert!(!menu.is_open());
}
