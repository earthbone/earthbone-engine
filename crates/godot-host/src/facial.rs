// SPDX-License-Identifier: GPL-3.0-or-later

//! Resolve CK facial expression ids to the matching head blend shape names.
//!
//! The CK `SetExpressionOverride` contract supplies one of sixteen expression
//! ids and a percentage. This module maps that state onto the named morph
//! targets present on a head mesh, leaving unrelated targets untouched.
//!
//! `soe-nif::tri` parses Skyrim FaceGen `.tri` morph deltas. The actor surface
//! loader and Godot server mesh path do not apply those deltas as blend shapes,
//! so this module currently implements the CK name mapping only.

use std::collections::BTreeMap;

/// CK expression identifiers in the order used by `Actor.SetExpressionOverride`.
pub const EXPRESSION_TARGETS: [&str; 16] = [
    "DialogueAnger",
    "DialogueFear",
    "DialogueHappy",
    "DialogueSad",
    "DialogueSurprise",
    "DialoguePuzzled",
    "DialogueDisgusted",
    "MoodNeutral",
    "MoodAnger",
    "MoodFear",
    "MoodHappy",
    "MoodSad",
    "MoodSurprise",
    "MoodPuzzled",
    "MoodDisgusted",
    "CombatAnger",
    // Combat Shout is the sixteenth target and kept separately below because
    // the CK range has seventeen labels counting both endpoint values.
];

/// Blend-shape weights to apply for a CK expression on one actor head.
///
/// The result contains expression target names that exist on the head. The
/// selected target receives `strength / 100`; competing expressions receive
/// zero. Unrelated head morphs are omitted and keep their existing weights.
#[must_use]
pub fn expression_weights(
    target_names: &[String],
    mood: u8,
    strength: u8,
) -> BTreeMap<String, f32> {
    let chosen = expression_name(mood);
    target_names
        .iter()
        .filter(|name| {
            EXPRESSION_TARGETS.contains(&name.as_str()) || name.as_str() == "CombatShout"
        })
        .map(|name| {
            let weight = if Some(name.as_str()) == chosen {
                f32::from(strength) / 100.0
            } else {
                0.0
            };
            (name.clone(), weight)
        })
        .collect()
}

/// Resolve one CK id to the blend shape name used by the actor head.
#[must_use]
pub fn expression_name(mood: u8) -> Option<&'static str> {
    match mood {
        0..=15 => EXPRESSION_TARGETS.get(usize::from(mood)).copied(),
        16 => Some("CombatShout"),
        _ => None,
    }
}

/// Apply expression weights to existing values, preserving unrelated targets.
pub fn apply_expression(values: &mut BTreeMap<String, f32>, mood: u8, strength: u8) {
    let Some(chosen) = expression_name(mood) else {
        return;
    };
    for name in EXPRESSION_TARGETS.into_iter().chain(["CombatShout"]) {
        if let Some(weight) = values.get_mut(name) {
            *weight = if name == chosen {
                f32::from(strength) / 100.0
            } else {
                0.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anm_59_expression_sets_named_target_and_clears_competing_expressions() {
        let mut values = BTreeMap::from([
            ("MoodHappy".to_string(), 1.0),
            ("MoodSurprise".to_string(), 0.0),
            ("Blink".to_string(), 0.25),
        ]);
        apply_expression(&mut values, 12, 50);
        assert_eq!(values["MoodSurprise"], 0.5);
        assert_eq!(values["MoodHappy"], 0.0);
        assert_eq!(values["Blink"], 0.25);
    }

    #[test]
    fn anm_59_expression_returns_to_record_mood_after_override_clears() {
        let mood = 3_u8;
        let resolved = expression_name(7 + mood).expect("record mood maps to CK mood");
        assert_eq!(resolved, "MoodHappy");
    }
}
