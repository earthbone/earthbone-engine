// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used)]

//! CMB-27 vanilla graph reaction check.
//!
//! `activeragdoll/src/main.cpp::StaggerActor` sets graph variables and sends
//! `staggerStart`. The local Nemesis master graph declares stagger events.

use std::path::Path;

use soe_bsa::Vfs;
use soe_godot_host::actor::{ActorAssets, MASTER_BEHAVIOR};
use soe_hkx::interp::Interpreter;

#[test]
fn cmb_27_vanilla_stagger_stop_returns_to_the_state_active_at_the_hit() {
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
            .expect("vanilla actor archive");
    }
    let assets = ActorAssets::load_with_behavior(
        &vfs,
        "meshes/actors/character/character assets/malebody_1.nif",
        MASTER_BEHAVIOR,
    )
    .expect("vanilla actor assets");
    let mut vm = Interpreter::new(
        &assets.graph,
        &assets.clips,
        assets.skeleton.reference_pose.clone(),
    );
    vm.variables.set_by_name("iRightHandType", 1.0);
    vm.variables.set_by_name("Speed", 1.0);
    assert!(assets.graph.data.event_id("moveStart").is_some());
    assert!(vm.send_event_named("moveStart"));
    for _ in 0..30 {
        vm.update(1.0 / 60.0);
    }
    let before_hit = vm.active_states();
    assert!(!before_hit.is_empty());
    vm.variables.set_by_name("staggerMagnitude", 1.0);
    assert!(vm.send_event_named("staggerStart"));
    let stagger_clip_active = (0..90).any(|_| {
        vm.update(1.0 / 60.0);
        vm.active_clips().iter().any(|(id, _)| {
            assets
                .graph
                .node(*id)
                .name
                .to_ascii_lowercase()
                .contains("stagger")
        })
    });
    assert!(stagger_clip_active);
    assert!(vm.send_event_named("staggerStop"));
    for _ in 0..180 {
        vm.update(1.0 / 60.0);
    }
    assert_eq!(vm.active_states(), before_hit);
}
