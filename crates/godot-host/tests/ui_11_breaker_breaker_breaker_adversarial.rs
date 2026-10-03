// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;

fn callback_game() -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("ConfigFixture")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(
            CONFIG_QUEST,
            "ConfigFixtureQuest",
            quest_flags::START_GAME_ENABLED,
        )
        .sub(
            soe_esm::Tag::new(b"VMAD"),
            vmad.write(soe_esm::Tag::new(b"QUST")),
        ),
    );
    let mut order = LoadOrder::new();
    order.add("ConfigFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);
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
fn ui_11_each_enabled_text_or_toggle_press_dispatches_one_full_id() {
    let mut game = callback_game();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(2);
    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page.add_toggle("Enabled", true, 0).unwrap();

    assert_eq!((text, toggle), (0x200, 0x201));
    assert!(page.select(&mut game, form, text));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x200));
    assert!(page.select(&mut game, form, toggle));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(2));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x201));
}

#[test]
fn ui_11_flag_updates_keep_hidden_rows_hidden_and_disabled_until_reenabled() {
    let mut game = callback_game();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let option = page
        .add_toggle("Enabled", false, OPTION_FLAG_DISABLED | 0x02 | 0x04)
        .unwrap();

    assert!(page.set_enabled(option, true));
    assert_eq!(page.rows()[0].flags, 0x06);
    assert!(!page.rows()[0].visible());
    assert!(!page.select(&mut game, form, option));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(0));

    assert!(page.set_enabled(option, false));
    assert!(page.set_enabled(option, true));
    assert_eq!(page.rows()[0].flags, 0x06);
    assert!(!page.select(&mut game, form, option));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(0));
}
