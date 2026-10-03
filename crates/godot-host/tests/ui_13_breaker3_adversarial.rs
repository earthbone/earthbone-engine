// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::input::{scan, Action, Bindings, ControlMap};
use soe_godot_host::mcm_menu::{ConfigPage, KeyConflictScanner, OptionKind};

const SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

fn empty_game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Ui13Breaker3.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

#[test]
fn ui_13_replaced_gameplay_key_does_not_leave_a_stale_vanilla_conflict() {
    let mut bindings = Bindings::vanilla_keyboard();
    bindings.apply_control_map(&ControlMap::parse("Forward\t0x23\t0xff\t0xff\t1\t1\t1\n"));
    let scanner = KeyConflictScanner::from_bindings(&bindings);

    assert_eq!(bindings.get(Action::Forward).unwrap().0[0].0, vec![0x23]);
    assert_eq!(scanner.conflict(0x23).control, Action::Forward.event_name());
    assert!(scanner.conflict(scan::W as i32).control.is_empty());
}

#[test]
fn ui_13_color_cancel_clears_dialog_and_reopen_resets_dialog_parameters() {
    let mut game = empty_game();
    let mut page = ConfigPage::new(0);
    let id = page.add_color("Tint", 0x123456, 0).unwrap();

    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    assert!(page.set_color_dialog_start_color(0x654321));
    assert!(page.set_color_dialog_default_color(0xabcdef));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
    assert!(!page.cancel_color_dialog(id + 1));
    assert!(page.cancel_color_dialog(id));
    assert_eq!(page.rows()[0].kind, OptionKind::Color { value: 0x123456 });
    assert!(page.open_color_dialog(&mut game, SCRIPT, id));
    let dialog = page.color_dialog().unwrap();
    assert_eq!((dialog.start_color, dialog.default_color), (-1, -1));
    assert!(page.cancel_color_dialog(id));
}
