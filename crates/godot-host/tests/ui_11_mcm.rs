#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind, OPTION_FLAG_DISABLED};
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{ArrayCreate, ArraySetElement, Assign, CallMethod, IAdd, PropSet};
use soe_papyrus::Value;

const CONFIG_QUEST: u32 = 0x800;
const SKYUI_SOURCE: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/skyui/dist/Data/Scripts/Source";

fn skyui_source(name: &str, variable: &str) -> String {
    let path = std::env::var_os(variable)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::Path::new(SKYUI_SOURCE).join(name));
    std::fs::read_to_string(path).unwrap()
}

fn function_body(source: &str, signature: &str) -> String {
    let lower = source.to_ascii_lowercase();
    let signature = signature.to_ascii_lowercase();
    let start = lower.find(&signature).unwrap();
    let body_start = source[start..].find('\n').unwrap() + start + 1;
    let body_end = lower[body_start..].find("endfunction").unwrap() + body_start;
    lower[body_start..body_end].to_owned()
}

fn normalized(source: &str) -> String {
    source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn skyui_fixture() -> Game {
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
    let child = ScriptBuilder::new("ConfigFixture", "SKI_ConfigBase")
        .variable("register_count", "Int", i(0))
        .variable("open_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Fixture Mod")])
                .ins(soe_papyrus::Opcode::ArrayCreate, vec![ident("pages"), i(1)])
                .ins(
                    soe_papyrus::Opcode::ArraySetElement,
                    vec![ident("pages"), i(0), s("General")],
                )
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
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), i(1)])
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
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let object = game.object_of_form(form);
    game.vm.get_var(object, "ConfigFixture", name).unwrap()
}

#[test]
fn ui_11_adds_text_and_toggle_rows_with_skyui_ids() {
    let mut page = ConfigPage::new(2);

    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page.add_toggle("Enabled", true, 0).unwrap();

    assert_eq!(text, 0x200);
    assert_eq!(toggle, 0x201);
    assert_eq!(
        page.rows(),
        &[
            soe_godot_host::mcm_menu::OptionRow {
                id: text,
                label: "Status".into(),
                kind: OptionKind::Text {
                    value: "Ready".into()
                },
                flags: 0,
            },
            soe_godot_host::mcm_menu::OptionRow {
                id: toggle,
                label: "Enabled".into(),
                kind: OptionKind::Toggle { checked: true },
                flags: 0,
            },
        ]
    );
}

#[test]
fn ui_11_updates_enabled_state_and_preserves_other_flags() {
    let mut page = ConfigPage::new(0);
    let option = page.add_toggle("Enabled", false, 0x04).unwrap();

    assert!(page.set_enabled(option, false));
    assert_eq!(page.rows()[0].flags, 0x05);
    assert!(!page.rows()[0].enabled());
    assert!(page.set_enabled(option, true));
    assert_eq!(page.rows()[0].flags, 0x04);
    assert!(page.rows()[0].enabled());
    assert!(!page.set_enabled(99, true));
    assert_eq!(OPTION_FLAG_DISABLED, 0x01);
}

#[test]
fn ui_11_selection_dispatches_once_and_rejects_disabled_rows() {
    let mut game = skyui_fixture();
    let script = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let mut page = ConfigPage::new(0);
    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page
        .add_toggle("Enabled", true, OPTION_FLAG_DISABLED)
        .unwrap();

    assert!(page.select(&mut game, script, text));
    assert!(!page.select(&mut game, script, toggle));
    assert!(!page.select(&mut game, script, 127));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0));
}

#[test]
fn ui_11_page_capacity_matches_skyui_slots() {
    let mut page = ConfigPage::new(1);
    for slot in 0..128 {
        assert_eq!(page.add_text("Entry", "", 0), Some(0x100 + slot));
    }
    assert_eq!(page.add_text("Overflow", "", 0), None);
}

#[test]
fn ui_11_ski_config_fixture_builds_rows_updates_flags_and_calls_once() {
    let mut game = skyui_fixture();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };

    assert_eq!(game.mcm_configs()[0].name, "Fixture Mod");
    assert!(game.mcm_open(form));
    assert_eq!(fixture_var(&mut game, "register_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "reset_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "open_count"), Value::Int(1));

    let mut page = ConfigPage::new(0);
    let text = page.add_text("Status", "Ready", 0).unwrap();
    let toggle = page.add_toggle("Enabled", true, 0).unwrap();
    assert_eq!((text, toggle), (0, 1));
    assert!(page.set_enabled(toggle, false));
    assert!(!page.rows()[1].enabled());
    assert!(page.set_enabled(toggle, true));
    assert!(page.rows()[1].enabled());
    assert!(page.set_enabled(toggle, false));

    assert!(page.select(&mut game, form, text));
    assert!(!page.select(&mut game, form, toggle));
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0));
}

#[test]
fn ui_11_unmodified_skyui_sources_define_text_toggle_flags_and_callback() {
    let base = skyui_source("SKI_ConfigBase.psc", "SOE_SKYUI_CONFIG_BASE");
    let manager = skyui_source("SKI_ConfigManager.psc", "SOE_SKYUI_CONFIG_MANAGER");
    let declarations = normalized(&base);

    assert!(declarations.contains("int property option_type_text = 0x02 autoreadonly"));
    assert!(declarations.contains("int property option_type_toggle = 0x03 autoreadonly"));
    assert!(declarations.contains("int property option_flag_disabled = 0x01 autoreadonly"));
    assert!(declarations.contains("int property option_flag_hidden = 0x02 autoreadonly"));

    let add_text = function_body(&base, "int function AddTextOption(");
    assert!(add_text.contains("return addoption(option_type_text, a_text, a_value, 0, a_flags)"));
    let add_toggle = function_body(&base, "int function AddToggleOption(");
    assert!(add_toggle
        .contains("return addoption(option_type_toggle, a_text, none, a_checked as int, a_flags)"));

    let set_flags = function_body(&base, "function SetOptionFlags(");
    assert!(set_flags.contains("int index = a_option % 0x100"));
    assert!(set_flags.contains("oldflags %= 0x100"));
    assert!(set_flags.contains("oldflags += a_flags * 0x100"));

    let select = function_body(&base, "function SelectOption(");
    assert!(select.contains("int option = a_index + _currentpagenum * 0x100"));
    assert!(select.contains("onoptionselect(option)"));

    let manager_select = function_body(&manager, "event OnOptionSelect(");
    assert!(manager_select.contains("_activeconfig.selectoption(optionindex)"));
}

#[test]
fn ui_11_runs_unmodified_skyui_manager_and_base_pex_fixture() {
    let (Some(base_path), Some(manager_path)) = (
        std::env::var_os("SOE_SKYUI_CONFIG_BASE_PEX"),
        std::env::var_os("SOE_SKYUI_CONFIG_MANAGER_PEX"),
    ) else {
        eprintln!(
            "skipping real SkyUI MCM execution: set SOE_SKYUI_CONFIG_BASE_PEX and SOE_SKYUI_CONFIG_MANAGER_PEX"
        );
        return;
    };
    let base = std::fs::read(base_path).unwrap();
    let manager = std::fs::read(manager_path).unwrap();
    let form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST,
    };
    let manager_form = GlobalFormId {
        file: 0,
        object: CONFIG_QUEST + 1,
    };
    let config_vmad = Vmad {
        scripts: vec![ScriptEntry::new("ConfigFixture")],
        ..Default::default()
    };
    let manager_vmad = Vmad {
        scripts: vec![ScriptEntry::new("SKI_ConfigManager")],
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
        .sub(Tag::new(b"VMAD"), config_vmad.write(Tag::new(b"QUST"))),
    );
    plugin.push_top(
        quest::build_record(CONFIG_QUEST + 1, "ManagerFixtureQuest", 0)
            .sub(Tag::new(b"VMAD"), manager_vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("ConfigFixture.esm", plugin).unwrap();
    let mut game = Game::new(order);

    let child = ScriptBuilder::new("ConfigFixture", "SKI_ConfigBase")
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Fixture Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(1)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .local("text_id", "Int")
                .local("toggle_id", "Int")
                .ins(
                    CallMethod,
                    vec![
                        s("AddTextOption"),
                        ident("self"),
                        ident("text_id"),
                        s("Status"),
                        s("Ready"),
                        i(0),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("AddToggleOption"),
                        ident("self"),
                        ident("toggle_id"),
                        s("Enabled"),
                        ident("::truevar"),
                        i(0),
                    ],
                )
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
    game.load_script(&manager).unwrap();
    game.load_script(&child).unwrap();
    game.start();
    assert!(game.mcm_open(form));
    assert!(game.mcm_select_page(0));

    let config_object = game.object_of_form(form);
    let option_flags = game
        .vm
        .get_var(config_object, "SKI_ConfigBase", "_optionFlagsBuf");
    let Some(Value::Array(option_flags)) = option_flags else {
        panic!("unmodified SKI_ConfigBase did not initialize its option buffer");
    };
    assert_eq!(
        &option_flags.items.borrow()[..2],
        &[Value::Int(0x02), Value::Int(0x03)]
    );

    let mut rows = ConfigPage::new(1);
    assert_eq!(rows.add_text("Status", "Ready", 0), Some(0x100));
    assert_eq!(rows.add_toggle("Enabled", true, 0), Some(0x101));
    assert!(rows.set_enabled(0x101, false));
    assert!(!rows.rows()[1].enabled());
    assert!(rows.set_enabled(0x101, true));
    assert!(rows.rows()[1].enabled());
    assert!(rows.set_enabled(0x101, false));
    assert!(!rows.select(&mut game, form, 0x101));

    let toggle_flags = game
        .vm
        .call(
            config_object,
            "SetOptionFlags",
            &[Value::Int(1), Value::Int(OPTION_FLAG_DISABLED as i32)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(toggle_flags).is_some());
    let Some(Value::Array(option_flags)) =
        game.vm
            .get_var(config_object, "SKI_ConfigBase", "_optionFlagsBuf")
    else {
        panic!("unmodified SKI_ConfigBase lost its option buffer");
    };
    assert_eq!(option_flags.items.borrow()[1], Value::Int(0x0103));
    let enable_toggle = game
        .vm
        .call(
            config_object,
            "SetOptionFlags",
            &[Value::Int(1), Value::Int(0)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(enable_toggle).is_some());
    let Some(Value::Array(option_flags)) =
        game.vm
            .get_var(config_object, "SKI_ConfigBase", "_optionFlagsBuf")
    else {
        panic!("unmodified SKI_ConfigBase lost its option buffer");
    };
    assert_eq!(option_flags.items.borrow()[1], Value::Int(0x03));

    let manager_object = game.object_of_form(manager_form);
    game.vm
        .set_var(
            manager_object,
            "SKI_ConfigManager",
            "_activeConfig",
            Value::Object(config_object),
        )
        .unwrap();
    game.send_event(
        manager_form,
        "OnOptionSelect",
        &[
            Value::string("SKICP_optionSelected"),
            Value::string(""),
            Value::Float(0.0),
            Value::None,
        ],
    );
    game.run_until_idle(64);
    assert_eq!(fixture_var(&mut game, "selection_count"), Value::Int(1));
    assert_eq!(fixture_var(&mut game, "selected_option"), Value::Int(0x100));
}
