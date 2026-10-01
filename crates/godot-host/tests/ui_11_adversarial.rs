#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, CallMethod, IAdd};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;

fn config_game() -> Game {
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
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("ConfigFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let base = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .function(
            FunctionBuilder::new("OpenConfig", "None")
                .ins(
                    CallMethod,
                    vec![s("SetPage"), ident("self"), ident("::nonevar"), s(""), i(-1)],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("SetPage", "None")
                .param("page", "String")
                .param("index", "Int")
                .ins(
                    CallMethod,
                    vec![
                        s("OnPageReset"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("page"),
                    ],
                )
                .build(),
        )
        .bytes();
    let child = ScriptBuilder::new("ConfigFixture", "SKI_ConfigBase")
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .build(),
        )
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
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
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
fn ui_11_first_named_page_uses_skyui_page_one_ids() {
    // SkyUI OpenConfig uses index -1 for the empty page, then named page zero
    // becomes current page number one in SKI_ConfigBase.SetPage.
    let mut page = ConfigPage::new(1);
    assert_eq!(page.add_text("Status", "Ready", 0), Some(0x100));
    assert_eq!(page.add_toggle("Enabled", true, 0), Some(0x101));
}

#[test]
fn ui_11_hidden_or_disabled_rows_never_dispatch_selection() {
    let mut game = config_game();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let hidden_text = page.add_text("Status", "Ready", 0x02).unwrap();
    let disabled_toggle = page
        .add_toggle("Enabled", true, OPTION_FLAG_DISABLED)
        .unwrap();

    assert!(!page.rows()[0].visible());
    assert!(!page.select(&mut game, form, hidden_text));
    assert!(!page.select(&mut game, form, disabled_toggle));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(0));

    assert!(page.set_enabled(disabled_toggle, true));
    assert!(page.select(&mut game, form, disabled_toggle));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x101));
}

#[test]
fn ui_11_repeated_selections_deliver_one_callback_per_press() {
    let mut game = config_game();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(1);
    let text = page.add_text("Status", "Ready", 0).unwrap();

    assert!(page.select(&mut game, form, text));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert!(page.select(&mut game, form, text));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(2));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x100));
}
