// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::ConfigPage;
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;

fn callback_game(with_script: bool) -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("ConfigFixture")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let record = quest::build_record(
        CONFIG_QUEST,
        "ConfigFixtureQuest",
        quest_flags::START_GAME_ENABLED,
    );
    plugin.push_top(if with_script {
        record.sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST")))
    } else {
        record
    });
    let mut order = LoadOrder::new();
    order.add("ConfigFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);

    if with_script {
        let script = ScriptBuilder::new("ConfigFixture", "Quest")
            .variable("selection_count", "Int", i(0))
            .variable("selected_option", "Int", i(-1))
            .function(
                FunctionBuilder::new("OnOptionSelect", "None")
                    .param("option", "Int")
                    .ins(
                        IAdd,
                        vec![ident("selection_count"), ident("selection_count"), i(1)],
                    )
                    .ins(Assign, vec![ident("selected_option"), ident("option")])
                    .build(),
            )
            .bytes();
        game.load_script(&script).unwrap();
    }
    game.start();
    game
}

fn fixture_var(game: &mut Game, name: &str) -> Value {
    let object = game.object_of_form(GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    });
    game.vm.get_var(object, "ConfigFixture", name).unwrap()
}

#[test]
fn ui_11_last_slot_on_named_page_dispatches_full_skyui_id_once() {
    let mut game = callback_game(true);
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(3);
    for slot in 0..127 {
        assert_eq!(page.add_text("Entry", "", 0), Some(0x300 + slot));
    }
    let toggle = page.add_toggle("Enabled", true, 0).unwrap();
    assert_eq!(toggle, 0x37f);
    assert!(page.select(&mut game, form, toggle));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x37f));
}

#[test]
fn ui_11_rejected_missing_callback_target_leaves_callback_state_unchanged() {
    let mut game = callback_game(false);
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let option = page.add_text("Status", "Ready", 0).unwrap();

    assert!(!page.select(&mut game, form, option));
}

#[test]
fn ui_11_page_id_limit_rejects_values_outside_papyrus_int_range() {
    let mut last_page = ConfigPage::new(0x7f_ffff);
    for _ in 0..128 {
        let id = last_page.add_text("Entry", "", 0).unwrap();
        assert!(i32::try_from(id).is_ok());
    }
    assert_eq!(last_page.add_text("Overflow", "", 0), None);

    let mut unrepresentable_page = ConfigPage::new(0x80_0000);
    assert_eq!(unrepresentable_page.add_text("Status", "Ready", 0), None);
}

#[test]
fn ui_11_enable_updates_preserve_hidden_flag_and_selection_stays_hidden() {
    let mut game = callback_game(true);
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let toggle = page.add_toggle("Enabled", false, 0x02).unwrap();

    assert!(page.set_enabled(toggle, true));
    assert_eq!(page.rows()[0].flags, 0x02);
    assert!(!page.rows()[0].visible());
    assert!(!page.select(&mut game, form, toggle));
    assert!(page.set_enabled(toggle, false));
    assert!(page.set_enabled(toggle, true));
    assert_eq!(page.rows()[0].flags, 0x02);
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(0));
}
