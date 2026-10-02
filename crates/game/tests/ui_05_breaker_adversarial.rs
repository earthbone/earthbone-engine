// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

#[test]
fn ui_05_waits_for_manager_ready_before_register_callback() {
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("PendingConfig")],
        ..Default::default()
    };
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        quest::build_record(0x800, "PendingConfigQuest", quest_flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
    );
    let mut order = LoadOrder::new();
    order.add("Ui05Pending.esm", plugin).unwrap();
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
        .bytes();
    let child = ScriptBuilder::new("PendingConfig", "SKI_ConfigBase")
        .variable("register_count", "Int", i(0))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Pending")])
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
        .bytes();
    game.load_script(&base).unwrap();
    game.load_script(&child).unwrap();
    game.start();

    let object = game.object_of_form(soe_esm::GlobalFormId {
        file: 0,
        object: 0x800,
    });
    let count = game
        .vm
        .vars_on(object, "PendingConfig")
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("register_count"))
        .unwrap()
        .1;
    assert_eq!(count, Value::Int(0));
}
