// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The whole animation chain on the installed game, without Godot.
//!
//! The vanilla human skeleton, the locomotion graph with its references
//! linked, every clip the archives hold for it, and a real skinned body mesh.
//! Each frame the interpreter's local pose is lifted into model space and
//! combined with the mesh's inverse bind transforms, giving one skinning
//! matrix per mesh bone in Godot's axes and metres. That is exactly what the
//! Godot node uploads, so a failure here is a failure in the maths rather than
//! in the engine binding.
//!
//! Gated on `SKYRIM_DATA`, skipping cleanly when it is unset.

use std::path::PathBuf;
use std::time::Instant;

use soe_bsa::Vfs;
use soe_godot_host::actor::ActorAssets;
use soe_godot_host::skinned::{skinning_transforms_into, Affine};
use soe_havok::interp::Interpreter;
use soe_havok::pose::to_model_space_into;
use soe_havok::QsTransform;

const BODY: &str = "meshes/actors/character/character assets/malebody_1.nif";

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

/// Just the archives a character needs, rather than the whole load order.
fn character_vfs() -> Option<Vfs> {
    let dir = data_dir()?;
    let mut vfs = Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("skipping: {} is not there", path.display());
            return None;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }
    Some(vfs)
}

fn finite(a: &Affine) -> bool {
    a.origin.iter().all(|v| v.is_finite())
        && a.basis.iter().all(|row| row.iter().all(|v| v.is_finite()))
}

#[test]
fn a_vanilla_body_skins_to_the_animated_skeleton() {
    let Some(vfs) = character_vfs() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let started = Instant::now();
    let assets = ActorAssets::load(&vfs, BODY).expect("the character loads");
    let load_time = started.elapsed();

    let (bones, matched) = assets.bone_totals();
    let unmatched: Vec<&String> = assets
        .surfaces
        .iter()
        .flat_map(|s| s.map.unmatched.iter())
        .collect();
    let by_case: usize = assets.surfaces.iter().map(|s| s.map.matched_by_case).sum();
    let triangles: usize = assets
        .surfaces
        .iter()
        .map(|s| s.arrays.triangle_count())
        .sum();
    let vertices: usize = assets
        .surfaces
        .iter()
        .map(|s| s.arrays.positions.len())
        .sum();

    println!(
        "{BODY}: {} skinned shapes, {vertices} vertices, {triangles} triangles",
        assets.surfaces.len()
    );
    println!(
        "  skeleton {} bones, mesh bones {bones}, matched {matched} ({by_case} by case), \
         unmatched {}",
        assets.skeleton.bones.len(),
        unmatched.len()
    );
    println!(
        "  graph {} nodes, {} clips named, {} loaded, {} missing, {} unresolved references, \
         loaded in {:.2} s",
        assets.graph.nodes.len(),
        assets.graph.clip_animation_names().len(),
        assets.clips.len(),
        assets.missing_clips.len(),
        assets.unresolved_references.len(),
        load_time.as_secs_f32()
    );
    for name in unmatched.iter().take(10) {
        println!("  unmatched bone {name}");
    }

    assert_eq!(
        assets.skeleton.bones.len(),
        99,
        "the vanilla human skeleton is 99 bones"
    );
    assert!(!assets.surfaces.is_empty(), "the body must have a skin");
    assert!(bones > 0, "the skin must name bones");
    assert_eq!(
        matched, bones,
        "every bone the body names should exist in the skeleton, missing {unmatched:?}"
    );

    // Drive the graph the way the game does, then skin every frame.
    let mut vm = Interpreter::new(
        &assets.graph,
        &assets.clips,
        assets.skeleton.reference_pose.clone(),
    );
    let dt = 1.0 / 60.0;
    let mut model: Vec<QsTransform> = Vec::new();
    let mut skins: Vec<Affine> = Vec::new();
    let surface = &assets.surfaces[0];

    // Two seconds of idle, then ask for a walk.
    for _ in 0..120 {
        vm.update(dt);
    }
    to_model_space_into(&assets.skeleton, vm.pose(), &mut model).expect("idle pose lifts");
    skinning_transforms_into(&surface.map, &surface.skin.bones, &model, &mut skins)
        .expect("idle skins");
    let idle_first = skins.clone();
    assert!(
        skins.iter().all(finite),
        "an idle skinning matrix is not finite"
    );

    if let Some(i) = vm.variables.index("Speed") {
        vm.variables.set_real(i, 1.0);
    }
    for name in &assets.graph.data.event_names {
        if name.eq_ignore_ascii_case("moveStart") {
            vm.send_event_named(name);
        }
    }

    let mut moved_frames = 0;
    let t0 = Instant::now();
    for _ in 0..180 {
        vm.update(dt);
        to_model_space_into(&assets.skeleton, vm.pose(), &mut model).expect("walk pose lifts");
        skinning_transforms_into(&surface.map, &surface.skin.bones, &model, &mut skins)
            .expect("walk skins");
        assert!(skins.iter().all(finite), "a skinning matrix is not finite");
        // A frame counts as moved when any bone's origin differs from where
        // the idle pose put it.
        if skins
            .iter()
            .zip(&idle_first)
            .any(|(a, b)| (0..3).any(|i| (a.origin[i] - b.origin[i]).abs() > 1e-4))
        {
            moved_frames += 1;
        }
    }
    let per_frame_us = t0.elapsed().as_secs_f64() * 1e6 / 180.0;

    // The root stays near the origin while the head sits about head height
    // above it, which is the cheapest check that the hierarchy composed the
    // right way up rather than collapsing or inverting.
    let head_at = assets
        .skeleton
        .bone_index("NPC Head [Head]")
        .expect("the skeleton names a head");
    let head_y = model[head_at].translation[2] * soe_godot_host::units::METRES_PER_UNIT;
    println!(
        "  180 walk frames: {moved_frames} moved, head at {head_y:.2} m, \
         {per_frame_us:.1} us per frame for pose plus skin"
    );

    assert!(
        moved_frames > 150,
        "the skin should move on almost every walk frame, only {moved_frames} did"
    );
    assert!(
        (1.0..2.5).contains(&head_y),
        "the head should sit at human height, got {head_y:.2} m"
    );
}
