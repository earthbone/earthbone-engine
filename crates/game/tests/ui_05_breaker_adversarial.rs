// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

fn form() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: 0x800,
    }
}

fn fixture() -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Config")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(0x800, "Config", quest_flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Config.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let base = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .auto_property("ModName", "String", s(""))
        .auto_property("Pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(
                    CallMethod,
                    vec![s("OnConfigInit"), ident("self"), ident("::nonevar")],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OpenConfig", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SetPage"),
                        ident("self"),
                        ident("::nonevar"),
                        s(""),
                        i(-1),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![s("OnConfigOpen"), ident("self"), ident("::nonevar")],
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
    let child = ScriptBuilder::new("Config", "SKI_ConfigBase")
        .variable("reset_count", "Int", i(0))
        .variable("last_page", "String", s("unset"))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Config")])
                .ins(ArrayCreate, vec![ident("pages"), i(1)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("Only page")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
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

fn variable(game: &mut Game, name: &str) -> Value {
    let object = game.object_of_form(form());
    game.vm
        .vars_on(object, "Config")
        .into_iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .unwrap()
        .1
}

#[test]
fn ui_05_open_resets_empty_page_then_declared_page_and_rejects_bad_indices() {
    let mut game = fixture();
    let configs = game.mcm_configs();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].pages, ["Only page"]);
    assert!(game.mcm_open(form()));
    assert_eq!(variable(&mut game, "reset_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "last_page"), Value::string(""));
    assert!(!game.mcm_select_page(1));
    assert!(!game.mcm_select_page(usize::MAX));
    assert!(game.mcm_select_page(0));
    assert_eq!(variable(&mut game, "last_page"), Value::string("Only page"));
}
