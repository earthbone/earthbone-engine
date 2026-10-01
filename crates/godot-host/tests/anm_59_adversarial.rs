// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use soe_godot_host::facial::{apply_expression, expression_name, expression_weights};

#[test]
fn anm_59_adversarial_covers_all_seventeen_ck_expression_ids() {
    let names = (0..=16)
        .map(|mood| expression_name(mood).unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(names[0], "DialogueAnger");
    assert_eq!(names[7], "MoodNeutral");
    assert_eq!(names[15], "CombatAnger");
    assert_eq!(names[16], "CombatShout");
    assert_eq!(expression_name(17), None);
}

#[test]
fn anm_59_adversarial_zero_strength_clears_only_competing_expression_morphs() {
    let mut values = BTreeMap::from([
        ("CombatShout".to_owned(), 0.7),
        ("MoodHappy".to_owned(), 0.8),
        ("Blink".to_owned(), 0.25),
    ]);
    apply_expression(&mut values, 16, 0);
    assert_eq!(values["CombatShout"], 0.0);
    assert_eq!(values["MoodHappy"], 0.0);
    assert_eq!(values["Blink"], 0.25);
}

#[test]
fn anm_59_adversarial_weights_include_only_targets_present_on_this_head() {
    let weights = expression_weights(
        &["MoodSad".into(), "CombatShout".into(), "Blink".into()],
        16,
        37,
    );
    assert_eq!(weights.len(), 2);
    assert_eq!(weights["CombatShout"], 0.37);
    assert_eq!(weights["MoodSad"], 0.0);
}
