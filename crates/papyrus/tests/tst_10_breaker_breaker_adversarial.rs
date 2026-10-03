// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

#[path = "tst_10_champollion.rs"]
mod champollion_harness;

#[test]
fn same_typed_parameters_with_different_names_must_be_reported() {
    let source = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.param first Int\n.param second Int\n.code\nRETURN first\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );
    let decoded = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.param second Int\n.param first Int\n.code\nRETURN second\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );

    assert!(
        !champollion_harness::differences(&source, &decoded).is_empty(),
        "Champollion emits each parameter name and type in declaration order"
    );
}
