// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use soe_papyrus::{pex, psc};

#[test]
fn scr_04_semicolons_inside_returned_strings_are_not_comments() {
    let source = "ScriptName Example\nString Function Text()\nReturn \"left;right\" ; trailing comment\nEndFunction\n";
    let compiled = psc::compile("Example.psc", source).unwrap();
    let parsed = pex::parse(&compiled).unwrap();
    let object = parsed.object();
    let function = object.states[0]
        .functions
        .iter()
        .find(|function| function.name == "Text")
        .unwrap();
    assert_eq!(
        function.code[0].args,
        vec![pex::Operand::Str("left;right".into())]
    );
}
