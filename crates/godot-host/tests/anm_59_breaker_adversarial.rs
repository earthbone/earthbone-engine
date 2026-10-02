// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::facial::expression_weights;

#[test]
fn anm_59_breaker_repeated_override_switch_clears_old_expression_and_keeps_other_morphs() {
    let targets = [
        "MoodHappy".to_owned(),
        "CombatShout".to_owned(),
        "Blink".to_owned(),
    ];
    let happy = expression_weights(&targets, 10, 100);
    assert_eq!(happy["MoodHappy"], 1.0);
    assert_eq!(happy["CombatShout"], 0.0);
    assert!(!happy.contains_key("Blink"));

    let shout = expression_weights(&targets, 16, 0);
    assert_eq!(shout["MoodHappy"], 0.0);
    assert_eq!(shout["CombatShout"], 0.0);
    assert!(!shout.contains_key("Blink"));
}

#[test]
fn anm_59_breaker_installed_head_morph_deltas_match_surface_vertex_order() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed head morph survey");
        return;
    };
    let mut vfs = soe_bsa::Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = data.join(archive);
        if !path.exists() {
            eprintln!(
                "{} is absent, skipping installed head morph survey",
                path.display()
            );
            return;
        }
        vfs.add_archive_path(&path).unwrap();
    }
    let assets = soe_godot_host::actor::ActorAssets::load(
        &vfs,
        "meshes/actors/character/character assets/femalehead.nif",
    )
    .unwrap();
    let shapes = assets
        .surfaces
        .iter()
        .flat_map(|surface| {
            surface
                .expression_shapes
                .iter()
                .map(move |shape| (surface, shape))
        })
        .collect::<Vec<_>>();
    assert!(
        !shapes.is_empty(),
        "installed head has no expression targets"
    );
    for (surface, shape) in shapes {
        assert_eq!(shape.positions.len(), surface.arrays.positions.len());
        assert!(shape
            .positions
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
        assert!(shape.normals.is_empty() || shape.normals.len() == surface.arrays.normals.len());
        assert!(shape
            .normals
            .iter()
            .flatten()
            .all(|value| value.is_finite()));
    }
}
