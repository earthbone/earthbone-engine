#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind, OPTION_FLAG_DISABLED};

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Fixture.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

#[test]
fn ui_11_adds_text_and_toggle_rows_with_skyui_ids() {
    let mut page = ConfigPage::new(2);

    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page.add_toggle("Enabled", true, 0).unwrap();

    assert_eq!(text, 0x200);
    assert_eq!(toggle, 0x201);
    assert_eq!(
        page.rows(),
        &[
            soe_godot_host::mcm_menu::OptionRow {
                id: text,
                label: "Status".into(),
                kind: OptionKind::Text {
                    value: "Ready".into()
                },
                flags: 0,
            },
            soe_godot_host::mcm_menu::OptionRow {
                id: toggle,
                label: "Enabled".into(),
                kind: OptionKind::Toggle { checked: true },
                flags: 0,
            },
        ]
    );
}

#[test]
fn ui_11_updates_enabled_state_and_preserves_other_flags() {
    let mut page = ConfigPage::new(0);
    let option = page.add_toggle("Enabled", false, 0x04).unwrap();

    assert!(page.set_enabled(option, false));
    assert_eq!(page.rows()[0].flags, 0x05);
    assert!(!page.rows()[0].enabled());
    assert!(page.set_enabled(option, true));
    assert_eq!(page.rows()[0].flags, 0x04);
    assert!(page.rows()[0].enabled());
    assert!(!page.set_enabled(99, true));
    assert_eq!(OPTION_FLAG_DISABLED, 0x01);
}

#[test]
fn ui_11_selection_dispatches_once_and_rejects_disabled_rows() {
    let mut game = game();
    let script = GlobalFormId {
        file: 0,
        object: 0x123,
    };
    let mut page = ConfigPage::new(0);
    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page
        .add_toggle("Enabled", true, OPTION_FLAG_DISABLED)
        .unwrap();

    assert!(page.select(&mut game, script, text));
    assert!(!page.select(&mut game, script, toggle));
    assert!(!page.select(&mut game, script, 127));
}

#[test]
fn ui_11_page_capacity_matches_skyui_slots() {
    let mut page = ConfigPage::new(1);
    for slot in 0..128 {
        assert_eq!(page.add_text("Entry", "", 0), Some(0x100 + slot));
    }
    assert_eq!(page.add_text("Overflow", "", 0), None);
}
