// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind};

const SCRIPT: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x123,
};

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Ui12.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    Game::new(order)
}

#[test]
fn ui_12_accept_and_cancel_require_an_open_dialog() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let slider = page.add_slider("Scale", 1.0, 0.0, 2.0, 0.5, 0).unwrap();
    let menu = page
        .add_menu("Mode", "A", vec!["A".into(), "B".into()], 0)
        .unwrap();

    assert!(!page.accept_slider(&mut game, SCRIPT, slider, 1.5));
    assert!(!page.accept_menu(&mut game, SCRIPT, menu, 1));
    assert!(!page.cancel_dialog(slider));
    assert!(!page.cancel_dialog(menu));
    assert_eq!(
        page.rows()[0].kind,
        OptionKind::Slider {
            value: 1.0,
            min_value: 0.0,
            max_value: 2.0,
            interval: 0.5,
        }
    );
    assert!(matches!(&page.rows()[1].kind, OptionKind::Menu { value, .. } if value == "A"));
}

#[test]
fn ui_12_cancel_closes_only_the_open_menu_dialog_once() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let menu = page
        .add_menu("Mode", "A", vec!["A".into(), "B".into()], 0)
        .unwrap();

    assert!(page.open_menu(&mut game, SCRIPT, menu).is_some());
    assert!(page.cancel_dialog(menu));
    assert!(!page.cancel_dialog(menu));
    assert!(!page.accept_menu(&mut game, SCRIPT, menu, 1));
    assert!(
        matches!(&page.rows()[0].kind, OptionKind::Menu { value, selected_index: 0, .. } if value == "A")
    );
}

#[test]
fn ui_12_slider_rounding_keeps_authored_minimum_and_maximum() {
    let mut game = game();
    let mut page = ConfigPage::new(0);
    let slider = page.add_slider("Scale", 0.25, 0.25, 1.0, 0.3, 0).unwrap();

    assert!(page.open_slider(&mut game, SCRIPT, slider).is_some());
    assert!(page.accept_slider(&mut game, SCRIPT, slider, 1.0));
    assert!(matches!(
        page.rows()[0].kind,
        OptionKind::Slider { value: 1.0, .. }
    ));
}
