// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

#[path = "tst_10_champollion.rs"]
mod champollion_harness;

use std::collections::BTreeMap;

#[test]
fn champollion_parameter_names_are_part_of_the_parameter_list() {
    let loaded = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.param amount Int\n.code\nRETURN amount\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );
    let decoded = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.param target Int\n.code\nRETURN target\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );

    assert!(
        !champollion_harness::differences(&loaded, &decoded).is_empty(),
        "AsmCoder emits parameter names and types, so renamed parameters must differ"
    );
}

#[test]
fn champollion_final_branch_label_is_not_counted_as_an_instruction() {
    let with_label = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.code\nJMP _label0\nRETURN None\n_label0\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );
    let without_label = champollion_harness::champollion_functions(
        ".object Example\n.state\n.function Tick\n.code\nJMP _label0\nRETURN None\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n",
    );

    assert!(
        champollion_harness::differences(&with_label, &without_label).is_empty(),
        "AsmCoder emits a final target label after the instruction list"
    );
}

#[test]
fn champollion_property_getter_and_setter_remain_distinct_functions() {
    let assembly = ".object Example\n.property Health Int\n.function get\n.code\nRETURN health\n.endCode\n.endFunction ;get\n.function set\n.param value Int\n.code\nRETURN None\n.endCode\n.endFunction ;set\n.endProperty\n.endObject\n";
    let functions = champollion_harness::champollion_functions(assembly);
    let keys = BTreeMap::from([
        (("example".into(), String::new(), "get".into(), "health".into()), ()),
        (("example".into(), String::new(), "set".into(), "health".into()), ()),
    ]);
    let actual: BTreeMap<_, _> = functions.keys().cloned().map(|key| (key, ())).collect();

    assert_eq!(actual, keys, "AsmCoder names accessors by kind and property");
}
