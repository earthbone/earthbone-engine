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
                .local("pages", "String[]")
                .ins(
                    IAdd,
                    vec![ident("register_count"), ident("register_count"), i(1)],
                )
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Example Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
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
    let configs = game.mcm_configs();
    assert_eq!(variable(&mut game, "register_count"), Value::Int(1));
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
fn ui_05_registration_callback_updates_the_first_presented_labels() {
    let mut game = fixture();
    let object = game.object_of_form(form());
    game.vm
        .set_property(
            object,
            "SKI_ConfigBase",
            "ModName",
            Value::string("Before registration"),
        )
        .unwrap();
    game.vm
        .set_property(object, "SKI_ConfigBase", "Pages", Value::None)
        .unwrap();
    let configs = game.mcm_configs();
    assert_eq!(configs[0].name, "Example Mod");
    assert_eq!(configs[0].pages, ["General", "Advanced"]);
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

use crate::support::ui_05_fixture as compiled;

#[test]
fn ui_05_executes_unmodified_skyui_base_when_pex_is_available() {
    let Some(fixture) = compiled::load() else {
        return;
    };
    let mut game = fixture.game();
    let object = game.object_of_form(form());
    let manager = game.object_of_form(GlobalFormId {
        file: 0,
        object: compiled::MANAGER,
    });
    let value = |game: &Game, name: &str| game.vm.get_var(object, compiled::CHILD, name).unwrap();
    assert_eq!(value(&game, "init_count"), Value::Int(1));
    assert_eq!(
        game.vm
            .get_property(object, "SKI_QuestBase", "CurrentVersion"),
        Some(Value::Int(1))
    );
    assert_eq!(game.mcm_configs()[0].pages, ["General", "Advanced"]);
    assert_eq!(
        game.vm.get_var(object, "SKI_ConfigBase", "_configID"),
        Some(Value::Int(0))
    );
    assert_eq!(
        game.vm.get_var(object, "SKI_ConfigBase", "_configManager"),
        Some(Value::Object(manager))
    );
    game.mcm_configs();
    assert_eq!(value(&game, "register_count"), Value::Int(1));
    assert!(game.mcm_open(form()));
    assert_eq!(value(&game, "sequence"), Value::Int(12));
    assert_eq!(value(&game, "last_page"), Value::string(""));
    assert!(
        matches!(game.vm.get_var(object, "SKI_ConfigBase", "_optionFlagsBuf"), Some(Value::Array(buffer)) if buffer.len() == 128)
    );
    let calls = game.take_mcm_ui_calls();
    let pages = calls
        .iter()
        .find(|call| call.target.ends_with(".setPageNames"))
        .unwrap();
    assert_eq!(
        pages.arguments,
        [Value::string("General"), Value::string("Advanced")]
    );
    assert!(game.mcm_select_page(1));
    assert_eq!(value(&game, "reset_count"), Value::Int(2));
    assert_eq!(value(&game, "last_page"), Value::string("Advanced"));
    assert_eq!(
        game.vm
            .get_property(object, "SKI_ConfigBase", "CurrentPage"),
        Some(Value::string("Advanced"))
    );
    assert_eq!(
        game.vm.get_var(object, "SKI_ConfigBase", "_currentPageNum"),
        Some(Value::Int(2))
    );
    assert!(!game.mcm_select_page(2));
    game.mcm_close();
    game.mcm_close();
    assert_eq!(value(&game, "close_count"), Value::Int(1));
    assert_eq!(value(&game, "sequence"), Value::Int(1213));
    assert!(
        matches!(game.vm.get_var(object, "SKI_ConfigBase", "_optionFlagsBuf"), Some(Value::Array(buffer)) if buffer.len() == 1)
    );
    assert!(
        !game
            .vm
            .log
            .messages()
            .iter()
            .any(|message| message.starts_with("error:")),
        "{:?}",
        game.vm.log.messages()
    );
    if let Some(directory) = std::env::var_os("SOE_UI_05_FIXTURE_DIR") {
        fixture.write(std::path::Path::new(&directory));
    }
}

#[test]
fn ui_05_mod_event_preserves_payload_and_unregisters_the_callback() {
    let mut game = fixture();
    let script = ScriptBuilder::new("Ui05ModEvent", "Form")
        .variable("count", "Int", i(0))
        .variable("payload", "String", s(""))
        .variable("number", "Float", soe_papyrus::asm::f(0.0))
        .variable("sender", "Form", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("Subscribe", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("RegisterForModEvent"),
                        ident("self"),
                        ident("::nonevar"),
                        s("Ui05Event"),
                        s("OnPayload"),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("Send", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SendModEvent"),
                        ident("self"),
                        ident("::nonevar"),
                        s("Ui05Event"),
                        s("page"),
                        soe_papyrus::asm::f(3.5),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("Unsubscribe", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("UnregisterForModEvent"),
                        ident("self"),
                        ident("::nonevar"),
                        s("Ui05Event"),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPayload", "None")
                .param("event", "String")
                .param("text", "String")
                .param("value", "Float")
                .param("source", "Form")
                .ins(IAdd, vec![ident("count"), ident("count"), i(1)])
                .ins(Assign, vec![ident("payload"), ident("text")])
                .ins(Assign, vec![ident("number"), ident("value")])
                .ins(Assign, vec![ident("sender"), ident("source")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.object_of_form(form());
    game.vm.attach(object, "Ui05ModEvent").unwrap();
    for method in ["Subscribe", "Subscribe", "Send"] {
        game.vm.call(object, method, &[]).unwrap();
        assert!(game.run_until_idle(64));
    }
    assert_eq!(
        game.vm.get_var(object, "Ui05ModEvent", "count"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(object, "Ui05ModEvent", "payload"),
        Some(Value::string("page"))
    );
    assert_eq!(
        game.vm.get_var(object, "Ui05ModEvent", "number"),
        Some(Value::Float(3.5))
    );
    assert_eq!(
        game.vm.get_var(object, "Ui05ModEvent", "sender"),
        Some(Value::Object(object))
    );
    for method in ["Unsubscribe", "Send"] {
        game.vm.call(object, method, &[]).unwrap();
        assert!(game.run_until_idle(64));
    }
    assert_eq!(
        game.vm.get_var(object, "Ui05ModEvent", "count"),
        Some(Value::Int(1))
    );
}

#[test]
fn ui_05_unmodified_manager_reset_registers_each_config_once_again() {
    let Some(fixture) = compiled::load() else {
        return;
    };
    let mut game = fixture.game();
    let object = game.object_of_form(form());
    let manager = game.object_of_form(GlobalFormId {
        file: 0,
        object: compiled::MANAGER,
    });
    assert_eq!(game.mcm_configs().len(), 1);
    let reset = game.vm.call(manager, "ForceReset", &[]).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(reset), Some(Value::None));
    game.mcm_configs();
    game.mcm_configs();
    assert_eq!(
        game.vm.get_var(object, compiled::CHILD, "init_count"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(object, compiled::CHILD, "register_count"),
        Some(Value::Int(2))
    );
    assert_eq!(
        game.vm
            .get_var(manager, "SKI_ConfigManager", "_configCount"),
        Some(Value::Int(1))
    );
    assert!(
        !game
            .vm
            .log
            .messages()
            .iter()
            .any(|message| message.starts_with("error:")),
        "{:?}",
        game.vm.log.messages()
    );
}

#[test]
fn ui_05_unmodified_manager_rejects_registration_while_the_list_is_open() {
    let Some(fixture) = compiled::load() else {
        return;
    };
    let mut game = fixture.game();
    let object = game.object_of_form(form());
    let manager = game.object_of_form(GlobalFormId {
        file: 0,
        object: compiled::MANAGER,
    });
    game.mcm_begin();
    let register = game
        .vm
        .call(
            manager,
            "RegisterMod",
            &[Value::Object(object), Value::string("Example Mod")],
        )
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(register), Some(Value::Int(-2)));
    game.mcm_close();
    let register = game
        .vm
        .call(
            manager,
            "RegisterMod",
            &[Value::Object(object), Value::string("Example Mod")],
        )
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(register), Some(Value::Int(0)));
}

#[test]
fn ui_05_unmodified_stopped_config_is_not_initialized_or_registered() {
    let Some(mut fixture) = compiled::load() else {
        return;
    };
    let index = fixture
        .plugin
        .records
        .iter()
        .position(|record| record.form_id == compiled::CONFIG)
        .unwrap();
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new(compiled::CHILD)],
        ..Default::default()
    };
    fixture.plugin.records[index] = quest::build_record(compiled::CONFIG, "StoppedConfig", 0)
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST")));
    let mut game = fixture.game();
    assert!(game.mcm_configs().is_empty());
    let object = game.object_of_form(form());
    assert_eq!(
        game.vm.get_var(object, compiled::CHILD, "init_count"),
        Some(Value::Int(0))
    );
    assert_eq!(
        game.vm.get_var(object, compiled::CHILD, "register_count"),
        Some(Value::Int(0))
    );
}
