// SPDX-License-Identifier: GPL-3.0-or-later

//! Resolve CK facial expressions and audio visemes to head morph targets.
//!
//! CK expression overrides map an expression id and percentage onto named
//! blend shapes. Audio visemes use race-authored facial target weights and
//! FaceGen TRI vertex deltas.
//!
//! `soe-nif::tri` parses Skyrim FaceGen `.tri` morph deltas. The actor surface
//! loader and Godot server mesh path do not apply those deltas as blend shapes,
//! so the viseme path returns evaluated poses for its caller.

use soe_audio::viseme::{morph_weights, Shape, VisemeTrack};
use soe_esm::{records::race::Race, GlobalFormId};
use soe_game::World;
use soe_nif::tri::HeadTri;
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

/// Resolve the expression blend-shape weights for an actor bound to a head.
///
/// The result includes available expression targets only. A missing actor or
/// an actor without a record mood produces no weights.
#[must_use]
pub fn bound_actor_expression_weights(
    world: &mut World,
    actor: GlobalFormId,
    target_names: &[String],
) -> BTreeMap<String, f32> {
    world
        .facial_expression(actor)
        .map_or_else(BTreeMap::new, |(mood, strength)| {
            // CK notes that combat expressions do not work on female actors.
            // An empty update leaves the actor's existing face weights intact.
            if mood >= 15
                && world
                    .actor_appearance(actor)
                    .is_ok_and(|appearance| appearance.female)
            {
                return BTreeMap::new();
            }
            expression_weights(target_names, mood, strength)
        })
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

/// Whether a TRI morph name is one of the CK actor expression targets.
#[must_use]
pub fn expression_name_for_target(name: &str) -> Option<&'static str> {
    EXPRESSION_TARGETS
        .into_iter()
        .chain(["CombatShout"])
        .find(|target| *target == name)
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
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
    use soe_game::{npc, World};

    const fn tag(bytes: &[u8; 4]) -> Tag {
        Tag::new(bytes)
    }

    fn female_actor_world() -> (World, GlobalFormId) {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(npc::build_record(
            0x100,
            "Female fixture",
            "Female fixture",
            npc::Config {
                flags: npc::flags::FEMALE,
                ..npc::Config::default()
            },
            npc::Stats::default(),
        ));
        plugin.push_top(
            Record::new(tag(b"ACHR"), 0x101)
                .sub(tag(b"NAME"), 0x100_u32.to_le_bytes())
                .sub(tag(b"DATA"), vec![0; 24]),
        );
        let mut order = LoadOrder::new();
        order.add("ANM59Female.esm", plugin).unwrap();
        (
            World::new(order),
            GlobalFormId {
                file: 0,
                object: 0x101,
            },
        )
    }

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

    #[test]
    fn anm_59_female_combat_override_leaves_head_unchanged_and_clear_resumes_mood() {
        let (mut world, actor) = female_actor_world();
        world.set_actor_value(actor, "mood", 3.0).unwrap();
        let targets = [
            "CombatAnger".to_string(),
            "CombatShout".to_string(),
            "MoodHappy".to_string(),
        ];

        world.set_expression_override(actor, 15, 100).unwrap();
        assert!(bound_actor_expression_weights(&mut world, actor, &targets).is_empty());

        world.clear_expression_override(actor).unwrap();
        assert_eq!(
            bound_actor_expression_weights(&mut world, actor, &targets),
            BTreeMap::from([
                ("CombatAnger".to_string(), 0.0),
                ("CombatShout".to_string(), 0.0),
                ("MoodHappy".to_string(), 1.0),
            ])
        );
    }
}

/// A sampled face pose in the vertex order of its FaceGen TRI.
#[derive(Debug, Clone, PartialEq)]
pub struct FacialPose {
    /// Target weights in the order declared by the race's `PHTN` record.
    pub weights: Vec<(String, f32)>,
    /// Positions evaluated from the TRI base vertices.
    pub positions: Vec<[f32; 3]>,
}

/// Evaluate one viseme sample against a race's authored facial targets.
///
/// Targets declared by the race but absent from the TRI remain in `weights`
/// and do not change geometry. This lets race data keep its own meaning when
/// a particular head TRI only carries a subset of the targets.
///
/// # Errors
/// Returns a NIF error when a TRI morph weight is non-finite or its vertex
/// count does not match the TRI.
pub fn sample_facial_pose(
    track: &VisemeTrack,
    seconds: f64,
    blend: f64,
    race: &Race,
    tri: &HeadTri,
) -> Result<FacialPose, soe_nif::Error> {
    let shapes = track.shapes_at(seconds, blend);
    sample_shape_weights(&shapes, race, tri)
}

/// Evaluate weighted viseme shapes against the race and TRI.
///
/// This entry point is useful for native LIP tracks, which already carry
/// target weights and do not use Rhubarb cue sampling.
///
/// # Errors
/// Returns a NIF error when a TRI morph weight is non-finite or its vertex
/// count does not match the TRI.
pub fn sample_shape_weights(
    shapes: &[(Shape, f32)],
    race: &Race,
    tri: &HeadTri,
) -> Result<FacialPose, soe_nif::Error> {
    let weights = morph_weights(shapes, race);
    let mut positions = tri.vertices().to_vec();
    for (name, weight) in &weights {
        if tri.morphs().iter().any(|morph| morph.name() == *name) {
            positions = tri.apply_morph(&positions, name, *weight)?;
        }
    }
    Ok(FacialPose {
        weights: weights
            .into_iter()
            .map(|(name, weight)| (name.to_owned(), weight))
            .collect(),
        positions,
    })
}
