// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_godot_host::input::{scan, Action, Bindings, ControlMap};
use soe_godot_host::mcm_menu::{ConfigPage, KeyConflictScanner, OptionKind};
use soe_papyrus::asm::{s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{PropSet, Return};

const CONFIG_FORM: u32 = 0x800;
const CONFIG: GlobalFormId = GlobalFormId {
    file: 0,
    object: CONFIG_FORM,
};

fn registered_config() -> Game {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Ui13Config")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(
            CONFIG_FORM,
            "Ui13ConfigQuest",
            quest_flags::START_GAME_ENABLED,
        )
        .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Ui13Config.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(
        &ScriptBuilder::new("SKI_ConfigBase", "Quest")
            .auto_property("ModName", "String", s(""))
            .auto_property("Pages", "String[]", soe_papyrus::asm::none())
            .bytes(),
    )
    .unwrap();
    game.load_script(
        &ScriptBuilder::new("Ui13Config", "SKI_ConfigBase")
            .function(
                FunctionBuilder::new("OnInit", "None")
                    .ins(
                        PropSet,
                        vec![
                            s("ModName"),
                            soe_papyrus::asm::ident("self"),
                            s("Dodge Mod"),
                        ],
                    )
                    .build(),
            )
            .function(
                FunctionBuilder::new("GetCustomControl", "String")
                    .param("keyCode", "Int")
                    .ins(Return, vec![s("Dodge")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    game.start();
    game
}

#[test]
fn ui_13_live_scanner_reads_the_active_vanilla_bindings() {
    let scanner =
        soe_godot_host::mcm_menu::KeyConflictScanner::from_bindings(&Bindings::vanilla_keyboard());

    assert_eq!(
        scanner.conflict(scan::SPACE as i32).control,
        Action::Jump.event_name()
    );
}

#[test]
fn ui_13_live_scanner_includes_controls_outside_the_gameplay_action_subset() {
    let mut bindings = Bindings::vanilla_keyboard();
    bindings.apply_control_map(&ControlMap::parse("Journal\t0x23\t0xff\t0xff\t1\t1\t1\n"));

    assert_eq!(
        KeyConflictScanner::from_bindings(&bindings)
            .conflict(0x23)
            .control,
        "Journal"
    );
    assert_eq!(
        KeyConflictScanner::from_bindings(&bindings)
            .conflict(scan::SPACE as i32)
            .control,
        "Jump"
    );
}

#[test]
fn ui_13_registered_config_get_custom_control_runs_through_live_mcm_lookup() {
    let mut game = registered_config();

    assert_eq!(game.mcm_configs()[0].name, "Dodge Mod");
    assert_eq!(
        game.mcm_custom_control(CONFIG, 44).as_deref(),
        Some("Dodge")
    );
}

#[test]
fn ui_13_live_remap_prefers_vanilla_then_queries_registered_configs() {
    let mut game = registered_config();
    let bindings = Bindings::vanilla_keyboard();
    let mut page = ConfigPage::new(0);
    let id = page.add_keymap("Custom dodge", 42, 0).unwrap();

    assert!(page.remap_key_live(&mut game, CONFIG, id, scan::SPACE as i32, &bindings));
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
    assert!(page.remap_key_live(&mut game, CONFIG, id, 44, &bindings));
    assert_eq!(page.rows()[0].kind, OptionKind::KeyMap { key_code: 42 });
}
