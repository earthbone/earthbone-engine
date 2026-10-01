// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

fn fixture() -> (Game, GlobalFormId) {
    let form = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("EdgeConfig")],
        ..Default::default()
    };
    let plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let record = quest::build_record(0x800, "EdgeConfigQuest", quest_flags::START_GAME_ENABLED)
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST")));
    let mut plugin = plugin;
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Ui05Breaker.esm", plugin).unwrap();
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
                .ins(CallMethod, vec![s("SetPage"), ident("self"), ident("::nonevar"), s(""), soe_papyrus::asm::i(-1)])
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
        .function(FunctionBuilder::new("CloseConfig", "None").build())
        .bytes();
    let child = ScriptBuilder::new("EdgeConfig", "SKI_ConfigBase")
        .variable("reset_count", "Int", soe_papyrus::asm::i(0))
        .variable("last_page", "String", s("unset"))
        .variable("::pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Edge Config")])
                .ins(ArrayCreate, vec![ident("::pages"), soe_papyrus::asm::i(2)])
                .ins(ArraySetElement, vec![ident("::pages"), soe_papyrus::asm::i(0), s("")])
                .ins(ArraySetElement, vec![ident("::pages"), soe_papyrus::asm::i(1), s("Second")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("::pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), soe_papyrus::asm::i(1)])
                .ins(Assign, vec![ident("last_page"), ident("page")])
                .build(),
        )
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();
    (game, form)
}

fn variable(game: &mut Game, form: GlobalFormId, name: &str) -> Value {
    let object = game.object_of_form(form);
    game.vm
        .vars_on(object, "EdgeConfig")
        .into_iter()
        .find(|(variable, _)| variable.eq_ignore_ascii_case(name))
        .unwrap()
        .1
}

#[test]
fn ui_05_empty_declared_page_is_rendered_and_reset_as_a_real_page() {
    let (mut game, form) = fixture();

    let config = game.mcm_configs().remove(0);
    assert_eq!(config.pages, ["", "Second"]);
    assert!(game.mcm_open(form));
    let opens_resets = variable(&mut game, form, "reset_count");
    assert_eq!(opens_resets, Value::Int(1));
    assert!(game.mcm_select_page(0));
    assert_eq!(variable(&mut game, form, "reset_count"), Value::Int(2));
    assert_eq!(variable(&mut game, form, "last_page"), Value::string(""));
}

#[test]
fn ui_05_out_of_range_page_does_not_send_a_reset_event() {
    let (mut game, form) = fixture();
    assert!(game.mcm_open(form));
    assert_eq!(variable(&mut game, form, "reset_count"), Value::Int(1));

    assert!(!game.mcm_select_page(2));
    assert_eq!(variable(&mut game, form, "reset_count"), Value::Int(1));
}
