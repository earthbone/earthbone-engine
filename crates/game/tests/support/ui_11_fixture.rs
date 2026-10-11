// SPDX-License-Identifier: GPL-3.0-or-later

//! Original SkyUI framework PEX with a config using its public option API.
//! Calls follow local SKI_ConfigBase.psc and ConfigMenuTemplate.psc.

use soe_papyrus::asm::{b, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{ArrayCreate, ArraySetElement, Assign, CallMethod, IAdd, PropSet};

use super::ui_05_fixture::{self as compiled, Fixture, CHILD};

pub fn load() -> Option<Fixture> {
    let mut fixture = compiled::load()?;
    fixture.scripts.retain(|(name, _)| name != CHILD);
    fixture.scripts.push((CHILD.into(), child()));
    Some(fixture)
}

fn child() -> Vec<u8> {
    let mut script = ScriptBuilder::new(CHILD, "SKI_ConfigBase")
        .variable("text_id", "Int", i(-1))
        .variable("toggle_id", "Int", i(-1))
        .variable("hidden_id", "Int", i(-1))
        .variable("selection_count", "Int", i(0))
        .variable("selected_option", "Int", i(-1))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("UI-11 Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnPageReset", "None")
                .param("page", "String")
                .ins(IAdd, vec![ident("reset_count"), ident("reset_count"), i(1)])
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
                        b(true),
                        i(1),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetCursorPosition"),
                        ident("self"),
                        ident("::nonevar"),
                        i(4),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("AddTextOption"),
                        ident("self"),
                        ident("hidden_id"),
                        s("Hidden"),
                        s("Secret"),
                        i(2),
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
                .ins(
                    CallMethod,
                    vec![
                        s("SetOptionFlags"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("text_id"),
                        i(1),
                        b(true),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetOptionFlags"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("toggle_id"),
                        i(1),
                        b(true),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetOptionFlags"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("toggle_id"),
                        i(0),
                        b(false),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetTextOptionValue"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("text_id"),
                        s("Selected"),
                        b(false),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetToggleOptionValue"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("toggle_id"),
                        b(false),
                        b(false),
                    ],
                )
                .build(),
        );
    for event in [
        "OnConfigInit",
        "OnConfigRegister",
        "OnPageReset",
        "OnConfigOpen",
        "OnConfigClose",
    ] {
        let counter = match event {
            "OnConfigInit" => "init_count",
            "OnConfigRegister" => "register_count",
            "OnPageReset" => "reset_count",
            "OnConfigOpen" => "open_count",
            _ => "close_count",
        };
        script = script.variable(counter, "Int", i(0));
        if matches!(event, "OnConfigInit" | "OnPageReset") {
            continue;
        }
        script = script.function(
            FunctionBuilder::new(event, "None")
                .ins(IAdd, vec![ident(counter), ident(counter), i(1)])
                .build(),
        );
    }
    for (name, flags) in [("EnableOptions", 0), ("DisableOptions", 1)] {
        script = script.function(
            FunctionBuilder::new(name, "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SetOptionFlags"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("text_id"),
                        i(flags),
                        b(true),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("SetOptionFlags"),
                        ident("self"),
                        ident("::nonevar"),
                        ident("toggle_id"),
                        i(flags),
                        b(false),
                    ],
                )
                .build(),
        );
    }
    script.bytes()
}
