// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::asm::{ident, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Opcode::{PropGet, PropSet, Return};
use soe_papyrus::Value;

use crate::common::installed;

#[test]
fn scr_75_game_days_passed_value_reads_and_writes_installed_global() {
    let Some(data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping GameDaysPassed.Value");
        return;
    };
    let mut game = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    game.load_scripts_from_data(&data);
    let form = game
        .form("GameDaysPassed")
        .expect("installed GameDaysPassed");
    let object = game.object_of_form(form);
    assert!(game.vm.scripts_on(object).is_empty());
    let script = ScriptBuilder::new("Scr75GlobalProbe", "")
        .function(
            FunctionBuilder::new("Read", "Float")
                .global()
                .param("target", "GlobalVariable")
                .local("result", "Float")
                .ins(
                    PropGet,
                    vec![ident("vAlUe"), ident("target"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .function(
            FunctionBuilder::new("WriteAndRead", "Float")
                .global()
                .param("target", "GlobalVariable")
                .param("value", "Float")
                .local("result", "Float")
                .ins(
                    PropSet,
                    vec![ident("VaLuE"), ident("target"), ident("value")],
                )
                .ins(
                    PropGet,
                    vec![ident("VALUE"), ident("target"), ident("result")],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.vm.load_bytes(&script).unwrap();
    let original = game.world_mut().global_value(form);
    let tid = game
        .vm
        .call_global("Scr75GlobalProbe", "Read", &[Value::Object(object)])
        .unwrap();
    assert!(game.vm.run_until_idle(10_000));
    assert_eq!(game.vm.thread_result(tid), Some(Value::Float(original)));
    let changed = original + 2.5;
    let tid = game
        .vm
        .call_global(
            "Scr75GlobalProbe",
            "WriteAndRead",
            &[Value::Object(object), Value::Float(changed)],
        )
        .unwrap();
    assert!(game.vm.run_until_idle(10_000));
    assert_eq!(game.vm.thread_result(tid), Some(Value::Float(changed)));
    assert_eq!(game.world_mut().global_value(form), changed);
    assert_eq!(
        game.vm.natives.calls.get("globalvariable.getvalue"),
        Some(&2)
    );
    assert_eq!(
        game.vm.natives.calls.get("globalvariable.setvalue"),
        Some(&1)
    );
    assert_eq!(game.vm.scripts_on(object), ["GlobalVariable"]);
    assert!(
        game.vm.log.messages().is_empty(),
        "{:?}",
        game.vm.log.messages()
    );
}
