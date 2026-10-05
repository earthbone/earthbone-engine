// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::ui::MenuState;

#[test]
fn native_menu_remains_stable_across_repeated_html_view_lifecycle_requests() {
    let mut menu = MenuState::default();
    for title in ["PrismaUI", "PrismaUI"] {
        assert!(menu.open(title));
        assert_eq!(menu.title(), Some(title));
        assert!(!menu.open("replacement view"));
        assert_eq!(menu.title(), Some(title));
        assert!(menu.close());
        assert!(!menu.close());
    }
}
