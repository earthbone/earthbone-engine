// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-05 lifecycle checks and SkyUI source contract checks.

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const QUEST: u32 = 0x800;

fn source_body(source: &str, signature: &str, end: &str) -> Option<String> {
    let source = source.to_ascii_lowercase();
    let start = source.find(&signature.to_ascii_lowercase())?;
    let body_start = source[start..].find('\n')? + start + 1;
    let body_end = source[body_start..].find(&end.to_ascii_lowercase())? + body_start;
    Some(
        source[body_start..body_end]
            .lines()
            .map(|line| line.split(';').next().unwrap_or_default().trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn fixture() -> Game {
    fixture_with_quests(1)
}

fn fixture_with_quests(count: u32) -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("MinimalConfig")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    for index in 0..count {
        plugin.push_top(
            quest::build_record(
                QUEST + index,
                &format!("MinimalConfigQuest{index}"),
                quest_flags::START_GAME_ENABLED,
            )
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
        .function(
            FunctionBuilder::new("CloseConfig", "None")
                .ins(
                    CallMethod,
                    vec![s("OnConfigClose"), ident("self"), ident("::nonevar")],
                )
                .build(),
        )
        .bytes();
    let child = ScriptBuilder::new("MinimalConfig", "SKI_ConfigBase")
        .variable("init_count", "Int", i(0))
        .variable("register_count", "Int", i(0))
        .variable("open_count", "Int", i(0))
        .variable("close_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .variable("sequence", "Int", i(0))
        .variable("last_page", "String", s("unset"))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Example Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigRegister", "None")
                .ins(
                    IAdd,
                    vec![ident("register_count"), ident("register_count"), i(1)],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigOpen", "None")
                .ins(IAdd, vec![ident("open_count"), ident("open_count"), i(1)])
                .ins(IMul, vec![ident("sequence"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(2)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigClose", "None")
                .ins(IAdd, vec![ident("close_count"), ident("close_count"), i(1)])
                .ins(IMul, vec![ident("sequence"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(3)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), i(1)])
                .ins(IMul, vec![ident("sequence"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(1)])
                .ins(Assign, vec![ident("last_page"), ident("page")])
                .build(),
        )
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();
    game
}

fn form() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: QUEST,
    }
}

fn variable(game: &mut Game, name: &str) -> Value {
    let object = game.object_of_form(form());
    game.vm
        .vars_on(object, "MinimalConfig")
        .into_iter()
        .find(|(variable, _)| variable.eq_ignore_ascii_case(name))
        .unwrap()
        .1
}

#[test]
fn ui_05_initializes_and_registers_modeled_config_once() {
    let mut game = fixture();
    assert_eq!(variable(&mut game, "register_count"), Value::Int(1));
    let configs = game.mcm_configs();
    assert_eq!(configs.len(), 1);
    assert_eq!(configs[0].name, "Example Mod");
    assert_eq!(variable(&mut game, "init_count"), Value::Int(1));
    game.mcm_configs();
    assert_eq!(variable(&mut game, "register_count"), Value::Int(1));
}

#[test]
fn ui_05_open_resets_empty_page_before_open_callback() {
    let mut game = fixture();
    assert!(game.mcm_open(form()));
    assert_eq!(variable(&mut game, "reset_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "last_page"), Value::string(""));
    assert_eq!(variable(&mut game, "open_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "sequence"), Value::Int(12));
}

#[test]
fn ui_05_renders_declared_pages_and_delivers_page_reset() {
    let mut game = fixture();
    assert_eq!(game.mcm_configs()[0].pages, ["General", "Advanced"]);
    assert!(game.mcm_open(form()));
    assert!(game.mcm_select_page(1));
    assert_eq!(variable(&mut game, "last_page"), Value::string("Advanced"));
    assert_eq!(variable(&mut game, "reset_count"), Value::Int(2));
    assert!(!game.mcm_select_page(2));
}

#[test]
fn ui_05_closes_config_once() {
    let mut game = fixture();
    assert!(game.mcm_open(form()));
    game.mcm_close();
    game.mcm_close();
    assert_eq!(variable(&mut game, "close_count"), Value::Int(1));
    assert_eq!(variable(&mut game, "sequence"), Value::Int(123));
}

#[test]
fn ui_05_registry_stops_at_skyui_limit() {
    let mut game = fixture_with_quests(129);
    assert_eq!(game.mcm_configs().len(), 128);
}

#[test]
fn ui_05_original_skyui_source_defines_lifecycle_order() {
    let Some(path) = std::env::var_os("SOE_SKYUI_CONFIG_BASE") else {
        eprintln!("skipping original SkyUI source check: SOE_SKYUI_CONFIG_BASE is unset");
        return;
    };
    let source = std::fs::read_to_string(path).unwrap();

    let init = source_body(&source, "event OnInit()", "endevent").unwrap();
    assert!(init.contains("ongamereload()"));

    let reload = source_body(&source, "event OnGameReload()", "endevent").unwrap();
    assert!(reload.contains("onconfiginit()"));
    assert!(reload
        .contains("registerformodevent(\"skicp_configmanagerready\", \"onconfigmanagerready\")"));

    let ready = source_body(&source, "event OnConfigManagerReady(", "endevent").unwrap();
    let register = ready.find("newmanager.registermod(self, modname)").unwrap();
    let callback = ready.find("onconfigregister()").unwrap();
    assert!(register < callback);

    let open = source_body(&source, "function OpenConfig()", "endfunction").unwrap();
    let reset = open.find("setpage(\"\", -1)").unwrap();
    let opened = open.find("onconfigopen()").unwrap();
    let pages = open.find(".setpagenames\", pages)").unwrap();
    assert!(reset < opened && opened < pages);

    let set_page = source_body(
        &source,
        "function SetPage(string a_page, int a_index)",
        "endfunction",
    )
    .unwrap();
    assert!(set_page.contains("onpagereset(a_page)"));
}

#[test]
fn ui_05_executes_unmodified_skyui_base_when_pex_is_available() {
    let Some(path) = std::env::var_os("SOE_SKYUI_CONFIG_BASE_PEX") else {
        eprintln!("skipping SkyUI PEX execution check: SOE_SKYUI_CONFIG_BASE_PEX is unset");
        return;
    };

    let base = std::fs::read(path).unwrap();
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Ui05ExecutableChild")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(
            QUEST,
            "Ui05ExecutableConfig",
            quest_flags::START_GAME_ENABLED,
        )
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Config.esm", plugin).unwrap();
    let mut game = Game::new(order);

    let child = ScriptBuilder::new("Ui05ExecutableChild", "SKI_ConfigBase")
        .variable("init_count", "Int", i(0))
        .variable("register_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Example Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigRegister", "None")
                .ins(
                    IAdd,
                    vec![ident("register_count"), ident("register_count"), i(1)],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), i(1)])
                .build(),
        )
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();

    let object = game.object_of_form(form());
    let variable = |game: &Game, name: &str| {
        game.vm
            .vars_on(object, "Ui05ExecutableChild")
            .into_iter()
            .find(|(variable, _)| variable.eq_ignore_ascii_case(name))
            .unwrap()
            .1
    };
    assert_eq!(variable(&game, "init_count"), Value::Int(1));
    assert_eq!(game.mcm_configs()[0].pages, ["General", "Advanced"]);
    assert_eq!(variable(&game, "register_count"), Value::Int(1));
    assert!(game.mcm_open(form()));
    assert!(game.mcm_select_page(1));
    assert_eq!(variable(&game, "reset_count"), Value::Int(2));
}
