// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

#[path = "tst_10_champollion.rs"]
mod champollion_harness;

#[test]
fn champollion_return_type_changes_must_be_reported() {
    let int_return = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.return Int\n.code\nRETURN value\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );
    let string_return = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.return String\n.code\nRETURN value\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );

    assert!(
        !champollion_harness::differences(&int_return, &string_return).is_empty(),
        "AsmCoder emits function return types, so a changed type must differ"
    );
}
