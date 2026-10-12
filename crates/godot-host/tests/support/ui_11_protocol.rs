// SPDX-License-Identifier: GPL-3.0-or-later

//! Synthetic SkyUI presentation follows SKI_ConfigBase.psc's FlushOptionBuffers
//! and SelectOption in the local SkyUI-Community source/scripts corpus.

use soe_esm::GlobalFormId;
use soe_game::Game;
use soe_godot_host::mcm_menu::{ConfigPage, OptionKind};
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::value::{ArrayRef, ElementKind};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::Value;

pub fn with_protocol(mut script: ScriptBuilder) -> ScriptBuilder {
    script = script.variable("_currentPageNum", "Int", i(0));
    for method in ["InvokeIntA", "InvokeStringA", "InvokeFloatA", "InvokeInt"] {
        script = script.function(
            FunctionBuilder::new(method, "None")
                .param("target", "String")
                .param("value", "None")
                .ins(
                    CallStatic,
                    vec![
                        s("UI"),
                        s(method),
                        ident("::nonevar"),
                        s("Journal Menu"),
                        ident("target"),
                        ident("value"),
                    ],
                )
                .build(),
        );
    }
    script
}

fn array(kind: ElementKind, values: Vec<Value>) -> Value {
    let array = ArrayRef::new(kind, 0);
    *array.items.borrow_mut() = values;
    Value::Array(array)
}

fn invoke(game: &mut Game, form: GlobalFormId, method: &str, target: &str, value: Value) {
    let object = game.object_of_form(form);
    let thread = game
        .vm
        .call(
            object,
            method,
            &[
                Value::string(&format!("_root.ConfigPanelFader.configPanel.{target}")),
                value,
            ],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(thread).is_some());
}

pub fn publish(game: &mut Game, form: GlobalFormId, page: &ConfigPage) {
    if game.mcm_page_number().is_none() {
        assert!(game.mcm_open(form));
    }
    let rows = page.rows();
    let number = rows.first().unwrap().id / 0x100;
    let object = game.object_of_form(form);
    game.vm
        .set_var(
            object,
            "SKI_ConfigBase",
            "_currentPageNum",
            Value::Int(i32::try_from(number).unwrap()),
        )
        .unwrap();
    let mut flags = Vec::new();
    let mut text = Vec::new();
    let mut strings = Vec::new();
    let mut numbers = Vec::new();
    for row in rows {
        let (kind, string, number) = match &row.kind {
            OptionKind::Text { value } => (2, value.clone(), 0.0),
            OptionKind::Toggle { checked } => (3, String::new(), f32::from(*checked)),
            _ => panic!("text/toggle protocol fixture"),
        };
        flags.push(Value::Int(kind | i32::try_from(row.flags << 8).unwrap()));
        text.push(Value::string(&row.label));
        strings.push(Value::string(&string));
        numbers.push(Value::Float(number));
    }
    for (method, target, kind, values) in [
        (
            "InvokeIntA",
            "setOptionFlagsBuffer",
            ElementKind::Int,
            flags,
        ),
        (
            "InvokeStringA",
            "setOptionTextBuffer",
            ElementKind::Str,
            text,
        ),
        (
            "InvokeStringA",
            "setOptionStrValueBuffer",
            ElementKind::Str,
            strings,
        ),
        (
            "InvokeFloatA",
            "setOptionNumValueBuffer",
            ElementKind::Float,
            numbers,
        ),
    ] {
        invoke(game, form, method, target, array(kind, values));
    }
    invoke(
        game,
        form,
        "InvokeInt",
        "flushOptionBuffers",
        Value::Int(i32::try_from(rows.len()).unwrap()),
    );
    assert_eq!(ConfigPage::from_game(game).unwrap().rows(), rows);
}
