// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, Value};

const FIRST: u32 = 0x800;

fn form(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    for object in FIRST..FIRST + 2 {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("Config")],
            ..Default::default()
        };
        plugin.push_top(
            quest::build_record(object, &format!("Config{object}"), quest_flags::START_GAME_ENABLED)
                .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Config.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let base = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .auto_property("ModName", "String", s(""))
        .auto_property("Pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(CallMethod, vec![s("OnConfigInit"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OpenConfig", "None")
                .ins(CallMethod, vec![s("SetPage"), ident("self"), ident("::nonevar"), s(""), i(-1)])
                .ins(CallMethod, vec![s("OnConfigOpen"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .function(
            FunctionBuilder::new("SetPage", "None")
                .param("page", "String")
                .param("index", "Int")
                .ins(CallMethod, vec![s("OnPageReset"), ident("self"), ident("::nonevar"), ident("page")])
                .build(),
        )
        .function(
            FunctionBuilder::new("CloseConfig", "None")
                .ins(CallMethod, vec![s("OnConfigClose"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .bytes();
    let child = ScriptBuilder::new("Config", "SKI_ConfigBase")
        .variable("init_count", "Int", i(0))
        .variable("register_count", "Int", i(0))
        .variable("open_count", "Int", i(0))
        .variable("close_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .variable("last_page", "String", s("unset"))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Adversarial Config")])
                .ins(ArrayCreate, vec![ident("::pages"), i(2)])
                .ins(ArraySetElement, vec![ident("::pages"), i(0), s("First")])
                .ins(ArraySetElement, vec![ident("::pages"), i(1), s("Second")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("::pages")])
                .build(),
        )
        .variable("::pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnConfigRegister", "None")
                .ins(IAdd, vec![ident("register_count"), ident("register_count"), i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigOpen", "None")
                .ins(IAdd, vec![ident("open_count"), ident("open_count"), i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigClose", "None")
                .ins(IAdd, vec![ident("close_count"), ident("close_count"), i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), i(1)])
                .ins(Assign, vec![ident("last_page"), ident("page")])
                .build(),
        )
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();
    game
}

fn variable(game: &mut Game, object: u32, name: &str) -> Value {
    let script_object = game.object_of_form(form(object));
    game.vm
        .vars_on(script_object, "Config")
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .unwrap()
        .1
}

#[test]
fn ui_05_repeated_discovery_does_not_repeat_initialization_or_registration() {
    let mut game = fixture();
    assert_eq!(game.mcm_configs().len(), 2);
    assert_eq!(game.mcm_configs().len(), 2);
    for object in FIRST..FIRST + 2 {
        assert_eq!(variable(&mut game, object, "init_count"), Value::Int(1));
        assert_eq!(variable(&mut game, object, "register_count"), Value::Int(1));
    }
}

#[test]
fn ui_05_page_list_is_read_from_the_script_property_each_time() {
    let mut game = fixture();
    let config_form = form(FIRST);
    let config_object = game.object_of_form(config_form);
    assert_eq!(game.mcm_configs()[0].pages, ["First", "Second"]);
    let pages = ArrayRef::new(ElementKind::Str, 1);
    pages.items.borrow_mut()[0] = Value::string("Replacement");
    game.vm
        .set_property(config_object, "SKI_ConfigBase", "Pages", Value::Array(pages))
        .unwrap();
    assert_eq!(game.mcm_configs()[0].pages, ["Replacement"]);
    assert_eq!(variable(&mut game, FIRST, "register_count"), Value::Int(1));
}

#[test]
fn ui_05_switching_config_closes_old_menu_and_resets_new_menu() {
    let mut game = fixture();
    assert!(game.mcm_open(form(FIRST)));
    assert!(game.mcm_open(form(FIRST + 1)));
    assert_eq!(variable(&mut game, FIRST, "close_count"), Value::Int(1));
    assert_eq!(variable(&mut game, FIRST + 1, "reset_count"), Value::Int(1));
    assert_eq!(variable(&mut game, FIRST + 1, "last_page"), Value::string(""));
    assert_eq!(variable(&mut game, FIRST + 1, "open_count"), Value::Int(1));
}
