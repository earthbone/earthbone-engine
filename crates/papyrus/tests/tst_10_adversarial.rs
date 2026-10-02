// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

#[path = "tst_10_champollion.rs"]
mod champollion_harness;

#[test]
fn champollion_keeps_multiple_property_accessors_with_the_same_emitted_name() {
    let assembly = ".object Example Parent\n.property First Int\n.function get\n.param self Object\n.code\nreturn 1\n.endCode\n.endFunction ;get\n.endProperty\n.property Second Int\n.function get\n.param self Object\n.code\nreturn 2\n.endCode\n.endFunction ;get\n.endProperty\n.endObject\n";
    let parsed = champollion_harness::champollion_functions(assembly);
    let getters = parsed
        .keys()
        .filter(|(_, state, name, _)| state.is_empty() && name == "get")
        .count();
    assert_eq!(
        getters, 2,
        "AsmCoder emits each property's accessor separately"
    );
    assert!(parsed.contains_key(&(
        "example".into(),
        String::new(),
        "get".into(),
        "first".into(),
    )));
    assert!(parsed.contains_key(&(
        "example".into(),
        String::new(),
        "get".into(),
        "second".into(),
    )));
}
