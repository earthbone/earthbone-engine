// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-27: the vanilla one-handed behavior graph plays its authored stagger
//! reaction, then returns to the state that was active before the hit.

use std::path::Path;

use soe_bsa::Vfs;
use soe_godot_host::actor::{ActorAssets, MASTER_BEHAVIOR};
use soe_hkx::interp::Interpreter;

#[test]
fn cmb_27_vanilla_hit_reaction_plays_and_returns_to_locomotion_without_damage() {
    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let mut vfs = Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&Path::new(&data_dir).join(archive))
            .unwrap();
    }
    let assets = ActorAssets::load_with_behavior(
        &vfs,
        "meshes/actors/character/character assets/malebody_1.nif",
        MASTER_BEHAVIOR,
    )
    .unwrap();
    assert!(assets.graph.data.event_id("staggerStart").is_some());
    assert!(assets
        .graph
        .data
        .variable_names
        .iter()
        .any(|variable| variable == "staggerMagnitude"));

    let mut vm = Interpreter::new(
        &assets.graph,
        &assets.clips,
        assets.skeleton.reference_pose.clone(),
    );
    vm.variables.set_by_name("iRightHandType", 1.0);
    vm.update(0.0);
    let locomotion = vm.active_states();
    assert!(!locomotion.is_empty(), "the graph must enter locomotion");

    vm.variables.set_by_name("staggerMagnitude", 1.0);
    assert!(vm.send_event_named("staggerStart"));
    let mut authored_stagger_clip = false;
    let mut state_changes = Vec::new();
    for _ in 0..90 {
        vm.update(1.0 / 60.0);
        state_changes.extend(vm.take_changes());
        authored_stagger_clip |= vm.active_clips().iter().any(|(id, _)| {
            assets
                .graph
                .node(*id)
                .name
                .to_ascii_lowercase()
                .contains("stagger")
        });
    }
    assert!(
        authored_stagger_clip,
        "the authored stagger clip must play, states: {:?}, clips: {:?}, changes: {:?}, unsupported: {:?}, unresolved: {:?}",
        vm.active_states(),
        vm.active_clips(),
        state_changes,
        vm.unsupported(),
        assets.unresolved_references
    );
    assert!(vm.send_event_named("staggerStop"));
    for _ in 0..180 {
        vm.update(1.0 / 60.0);
    }
    assert_eq!(vm.active_states(), locomotion);
    // This graph-only reaction has no Game or World handle, so it cannot
    // resolve the hit or apply health damage a second time.
}
