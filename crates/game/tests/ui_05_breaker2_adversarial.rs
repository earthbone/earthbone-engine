// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

fn read_count(game: &mut Game, object: GlobalFormId, name: &str) -> Value {
    let vm_object = game.object_of_form(object);
    game.vm
        .vars_on(vm_object, "RepeatedConfig")
        .into_iter()
        .find(|(variable, _)| variable.eq_ignore_ascii_case(name))
        .unwrap()
        .1
}

#[test]
fn ui_05_repeated_registration_queries_do_not_repeat_register_callback() {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("RepeatedConfig")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(0x800, "RepeatedConfigQuest", quest_flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("RepeatedConfig.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let base = ScriptBuilder::new("SKI_ConfigBase", "Quest")
        .auto_property("ModName", "String", s(""))
        .auto_property("Pages", "String[]", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(CallMethod, vec![s("OnConfigInit"), ident("self"), ident("::nonevar")])
                .build(),
        )
        .bytes();
    let child = ScriptBuilder::new("RepeatedConfig", "SKI_ConfigBase")
        .variable("register_count", "Int", i(0))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Repeated")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnConfigRegister", "None")
                .ins(IAdd, vec![ident("register_count"), ident("register_count"), i(1)])
                .build(),
        )
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();
    let object = GlobalFormId { file: 0, object: 0x800 };
    assert_eq!(game.mcm_configs().len(), 1);
    assert_eq!(game.mcm_configs().len(), 1);
    assert_eq!(read_count(&mut game, object, "register_count"), Value::Int(1));
}

#[test]
fn ui_05_page_reset_is_rejected_before_open_and_for_undeclared_index() {
    let mut order = LoadOrder::new();
    order
        .add("EmptyConfig.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut game = Game::new(order);
    assert!(!game.mcm_select_page(0));
    assert!(!game.mcm_open(GlobalFormId { file: 0, object: 0x800 }));
}
