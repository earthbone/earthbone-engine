// SPDX-License-Identifier: GPL-3.0-or-later

//! Original installed SkyUI framework with an independently authored config.

use super::compiled::{self, Fixture, CHILD};
use soe_papyrus::asm::{b, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{ArrayCreate, ArraySetElement, Assign, CallMethod, IAdd, PropSet};

fn call(
    function: FunctionBuilder,
    method: &str,
    args: Vec<soe_papyrus::pex::Operand>,
) -> FunctionBuilder {
    let mut operands = vec![s(method), ident("self"), ident("::nonevar")];
    operands.extend(args);
    function.ins(CallMethod, operands)
}

pub fn load(states: bool) -> Option<Fixture> {
    let mut fixture = compiled::load()?;
    let mut child = ScriptBuilder::new(CHILD, "SKI_ConfigBase")
        .variable("color_id", "Int", i(-1))
        .variable("key_id", "Int", i(-1))
        .variable("color_open_count", "Int", i(0))
        .variable("color_accept_count", "Int", i(0))
        .variable("key_count", "Int", i(0))
        .variable("last_key", "Int", i(-1))
        .variable("conflict_control", "String", s(""))
        .variable("conflict_name", "String", s(""))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(PropSet, vec![s("ModName"), ident("self"), s("UI-13 Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(1)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        )
        .function(
            FunctionBuilder::new("GetCustomControl", "String")
                .param("key", "Int")
                .ins(soe_papyrus::Opcode::Return, vec![s("ModAction")])
                .build(),
        );
    let reset = FunctionBuilder::new("OnPageReset", "None").param("page", "String");
    let reset = if states {
        call(
            call(
                reset,
                "AddColorOptionST",
                vec![s("TintState"), s("Tint"), i(0x112233), i(0)],
            ),
            "AddKeyMapOptionST",
            vec![s("KeyState"), s("Binding"), i(42), i(0)],
        )
    } else {
        reset
            .ins(
                CallMethod,
                vec![
                    s("AddColorOption"),
                    ident("self"),
                    ident("color_id"),
                    s("Tint"),
                    i(0x112233),
                    i(0),
                ],
            )
            .ins(
                CallMethod,
                vec![
                    s("AddKeyMapOption"),
                    ident("self"),
                    ident("key_id"),
                    s("Binding"),
                    i(42),
                    i(0),
                ],
            )
    };
    child = child.function(reset.build());
    let mut open = FunctionBuilder::new(
        if states {
            "OnColorOpenST"
        } else {
            "OnOptionColorOpen"
        },
        "None",
    );
    if !states {
        open = open.param("option", "Int");
    }
    open = open.ins(
        IAdd,
        vec![ident("color_open_count"), ident("color_open_count"), i(1)],
    );
    open = call(
        call(open, "SetColorDialogStartColor", vec![i(0x123456)]),
        "SetColorDialogDefaultColor",
        vec![i(0xabcdef)],
    );
    let mut accept = FunctionBuilder::new(
        if states {
            "OnColorAcceptST"
        } else {
            "OnOptionColorAccept"
        },
        "None",
    );
    if !states {
        accept = accept.param("option", "Int");
    }
    accept = accept.param("color", "Int").ins(
        IAdd,
        vec![
            ident("color_accept_count"),
            ident("color_accept_count"),
            i(1),
        ],
    );
    accept = if states {
        call(
            accept,
            "SetColorOptionValueST",
            vec![ident("color"), b(false), s("")],
        )
    } else {
        call(
            accept,
            "SetColorOptionValue",
            vec![ident("option"), ident("color"), b(false)],
        )
    };
    let mut remap = FunctionBuilder::new(
        if states {
            "OnKeyMapChangeST"
        } else {
            "OnOptionKeyMapChange"
        },
        "None",
    );
    if !states {
        remap = remap.param("option", "Int");
    }
    remap = remap
        .param("key", "Int")
        .param("control", "String")
        .param("modName", "String")
        .ins(IAdd, vec![ident("key_count"), ident("key_count"), i(1)])
        .ins(Assign, vec![ident("last_key"), ident("key")])
        .ins(Assign, vec![ident("conflict_control"), ident("control")])
        .ins(Assign, vec![ident("conflict_name"), ident("modName")]);
    remap = if states {
        call(
            remap,
            "SetKeyMapOptionValueST",
            vec![ident("key"), b(false), s("")],
        )
    } else {
        call(
            remap,
            "SetKeyMapOptionValue",
            vec![ident("option"), ident("key"), b(false)],
        )
    };
    if states {
        child = child
            .function_in("TintState", open.build())
            .function_in("TintState", accept.build())
            .function_in("KeyState", remap.build());
    } else {
        child = child
            .function(open.build())
            .function(accept.build())
            .function(remap.build());
    }
    fixture.scripts.retain(|(name, _)| name != CHILD);
    fixture.scripts.push((CHILD.into(), child.bytes()));
    Some(fixture)
}
