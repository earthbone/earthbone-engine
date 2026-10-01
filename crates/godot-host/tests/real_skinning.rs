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
use soe_hkx::interp::Interpreter;
use soe_hkx::pose::to_model_space_into;
use soe_hkx::QsTransform;

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
        "Skyrim - Textures0.bsa",
        "Skyrim - Textures1.bsa",
        "Skyrim - Textures2.bsa",
        "Skyrim - Textures3.bsa",
        "Skyrim - Textures4.bsa",
        "Skyrim - Textures5.bsa",
        "Skyrim - Textures6.bsa",
        "Skyrim - Textures7.bsa",
        "Skyrim - Textures8.bsa",
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

    // Before animating: does the bind pose alone skin to something sane? If
    // this is wrong the mesh, the skeleton or the inverse bind is at fault. If
    // it is right, anything wrong later is the animation's doing.
    {
        let mut bind = Vec::new();
        to_model_space_into(&assets.skeleton, &assets.skeleton.reference_pose, &mut bind)
            .expect("the reference pose lifts");
        let surface = &assets.surfaces[0];
        let mut bind_skins = Vec::new();
        skinning_transforms_into(&surface.map, &surface.skin, &bind, &mut bind_skins)
            .expect("the bind pose skins");
        let worst = bind_skins
            .iter()
            .map(|a| (0..3).map(|i| a.origin[i].abs()).fold(0.0f32, f32::max))
            .fold(0.0f32, f32::max);
        // Not an error term. This is the skin to model offset the vertices
        // still need, the same for every bone, so it is the shape's own
        // transform read back. What matters is the spread below it.
        println!("  bind pose: common skin to model offset {worst:.4} m");
        println!(
            "  shape node transform: {:?}",
            assets.surfaces[0].node_offset
        );
        let mut residual: Vec<(f32, &str)> = bind_skins
            .iter()
            .zip(&surface.skin.bones)
            .map(|(a, b)| {
                let d = (0..3).map(|i| a.origin[i].abs()).fold(0.0f32, f32::max);
                (d, b.name.as_str())
            })
            .collect();
        residual.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (d, name) in residual.iter().take(6) {
            println!("    {name:<26} residual {d:.4} m");
        }

        // Asserted now rather than reported, but not against the identity.
        // A Special Edition shape stores its vertices in skin space, so at the
        // reference pose every bone must produce the *same* transform, the one
        // that carries skin space into model space. Bones disagreeing is the
        // real fault signature: it read 1.72 m of spread until 2026-09-18,
        // when reading `NiSkinData`'s transposed rotations fixed it.
        let first = bind_skins[0];
        let spread = bind_skins
            .iter()
            .map(|a| {
                let o = (0..3)
                    .map(|i| (a.origin[i] - first.origin[i]).abs())
                    .fold(0.0f32, f32::max);
                let b = (0..3)
                    .flat_map(|r| (0..3).map(move |c| (r, c)))
                    .map(|(r, c)| (a.basis[r][c] - first.basis[r][c]).abs())
                    .fold(0.0f32, f32::max);
                o.max(b)
            })
            .fold(0.0f32, f32::max);
        println!("  bind pose: bones disagree by at most {spread:.6}");
        assert!(
            spread < 1e-3,
            "at the reference pose every bone must give the same transform, spread is {spread}"
        );
    }

    // Scale is the cheapest way a bone collapses a limb to a sliver, so
    // report its range across the whole skeleton rather than trust it.
    let scale_range = |pose: &[QsTransform]| {
        pose.iter()
            .flat_map(|t| t.scale[..3].iter().copied())
            .fold((f32::MAX, f32::MIN), |(lo, hi), s| (lo.min(s), hi.max(s)))
    };
    let (rlo, rhi) = scale_range(&assets.skeleton.reference_pose);
    println!("  reference pose scale range {rlo:.3} to {rhi:.3}");

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
    skinning_transforms_into(&surface.map, &surface.skin, &model, &mut skins).expect("idle skins");
    let idle_first = skins.clone();
    let (ilo, ihi) = scale_range(vm.pose());
    println!("  idle pose scale range {ilo:.3} to {ihi:.3}");
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
        skinning_transforms_into(&surface.map, &surface.skin, &model, &mut skins)
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

#[test]
fn live_skin_overrides_rebuild_authored_models_without_inventory() {
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
    use soe_godot_host::actor::AppearanceCache;
    let Some(vfs) = character_vfs() else { return };
    let assets = ActorAssets::load(&vfs, BODY).unwrap();
    let tag = Tag::new;
    let id = |object| GlobalFormId { file: 0, object };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag(b"NPC_"), 0x100)
            .sub(tag(b"ACBS"), vec![0; 24])
            .sub(tag(b"RNAM"), 0x101_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(tag(b"RACE"), 0x101).sub(tag(b"WNAM"), 0x200_u32.to_le_bytes()));
    for (index, path) in [
        "actors/character/character assets/malebody_1.nif",
        "actors/character/character assets/malehands_1.nif",
        "missing-skin-model.nif",
    ]
    .iter()
    .enumerate()
    {
        let offset = index as u32;
        plugin.push_top(
            Record::new(tag(b"ARMO"), 0x200 + offset)
                .sub(tag(b"MODL"), (0x300 + offset).to_le_bytes()),
        );
        plugin.push_top(
            Record::new(tag(b"ARMA"), 0x300 + offset)
                .sub(tag(b"RNAM"), 0x101_u32.to_le_bytes())
                .sub(tag(b"BOD2"), 4_u32.to_le_bytes())
                .sub(tag(b"DNAM"), vec![0; 12])
                .zsub(tag(b"MOD2"), path),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skin.esm", plugin).unwrap();
    let mut world = soe_game::World::new(order);
    let actor = world.place_at(id(0x100), id(0x14), false);
    let mut cache = AppearanceCache::default();
    let first = cache
        .prepare(&mut world, actor, &vfs, &assets.skeleton)
        .unwrap()
        .unwrap();
    let inventory = world.reference(actor).unwrap().inventory.clone();
    let mut wrong_rig = assets.skeleton.clone();
    for bone in &mut wrong_rig.bones {
        bone.name.insert_str(0, "wrong:");
    }
    assert!(
        matches!(AppearanceCache::default().prepare(&mut world, actor, &vfs, &wrong_rig),
        Err(soe_godot_host::actor::ActorError::RigMismatch { bones }) if !bones.is_empty())
    );

    let triangles = |surfaces: &[soe_godot_host::actor::ActorSurface]| {
        surfaces
            .iter()
            .map(|surface| surface.arrays.triangle_count())
            .sum::<usize>()
    };
    assert_eq!(triangles(&first.surfaces), triangles(&assets.surfaces));
    // Write the cost threshold before measuring: a cached single actor view
    // must average less than 2 ms, including canonical record resolution.
    let begin = Instant::now();
    for _ in 0..1000 {
        assert!(cache
            .prepare(&mut world, actor, &vfs, &assets.skeleton)
            .unwrap()
            .is_none());
    }
    let micros = begin.elapsed().as_secs_f64() * 1e6 / 1000.0;
    assert!(micros < 2000.0, "cached appearance costs {micros} us");
    world.set_race_skin(id(0x101), Some(id(0x201))).unwrap();
    let hands = cache
        .prepare(&mut world, actor, &vfs, &assets.skeleton)
        .unwrap()
        .unwrap();
    assert!(triangles(&hands.surfaces) > 0);
    assert_ne!(triangles(&hands.surfaces), triangles(&first.surfaces));
    world
        .set_actor_base_skin(id(0x100), Some(id(0x202)))
        .unwrap();
    assert!(cache
        .prepare(&mut world, actor, &vfs, &assets.skeleton)
        .is_err());
    assert!(
        cache
            .prepare(&mut world, actor, &vfs, &assets.skeleton)
            .unwrap()
            .is_none(),
        "an unchanged failed view must not load or report its error again"
    );
    assert!(
        triangles(&hands.surfaces) > 0,
        "failed preparation preserves old geometry"
    );
    world
        .set_actor_base_skin(id(0x100), Some(id(0x200)))
        .unwrap();
    let body = cache
        .prepare(&mut world, actor, &vfs, &assets.skeleton)
        .unwrap()
        .unwrap();
    assert_eq!(triangles(&body.surfaces), triangles(&first.surfaces));
    world.set_actor_base_skin(id(0x100), None).unwrap();
    world.set_race_skin(id(0x101), None).unwrap();
    let empty = cache
        .prepare(&mut world, actor, &vfs, &assets.skeleton)
        .unwrap()
        .unwrap();
    assert!(empty.surfaces.is_empty());
    assert_eq!(world.reference(actor).unwrap().inventory, inventory);
    assert!(world.actor(actor).unwrap().equipped.is_empty());
    eprintln!("live body/hand/empty skin refresh passed, cached view {micros:.1} us, no new nodes");
}

#[test]
fn installed_race_skins_assemble_multiple_authored_addon_models() {
    use soe_esm::{LoadOrder, Plugin, Tag};
    use soe_game::appearance::ActorAppearance;
    use std::collections::BTreeSet;
    let (Some(vfs), Some(data)) = (character_vfs(), data_dir()) else {
        return;
    };
    let assets = ActorAssets::load(&vfs, BODY).unwrap();
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut races = 0;
    let mut paths = BTreeSet::new();
    let mut shapes = 0;
    for (race, record) in order.winners_of(Tag::new(b"RACE")) {
        if !matches!(
            record.editor_id().as_deref(),
            Some("NordRace" | "BretonRace" | "HighElfRace" | "KhajiitRace" | "ArgonianRace")
        ) {
            continue;
        }
        let view = ActorAppearance::from_order(&order, None, Some(race), &BTreeSet::new()).unwrap();
        assert!(
            view.parts.len() >= 3,
            "body, hands and feet must be separate authored addons"
        );
        paths.extend(view.parts.iter().filter_map(|part| part.model.clone()));
        let surfaces = ActorAssets::appearance_surfaces(&vfs, &view, &assets.skeleton).unwrap();
        assert!(surfaces.len() >= 3);
        let unmatched: BTreeSet<_> = surfaces
            .iter()
            .flat_map(|surface| surface.map.unmatched.iter())
            .collect();
        eprintln!("{:?} rig coverage: {unmatched:?}", record.editor_id());
        if !unmatched.is_empty() {
            eprintln!(
                "selected beast models: {:?}",
                view.parts
                    .iter()
                    .filter_map(|part| part.model.as_deref())
                    .collect::<Vec<_>>()
            );
        }
        shapes += surfaces.len();
        races += 1;
    }
    assert_eq!(races, 5);
    assert!(
        paths.len() >= 5,
        "survey must include distinct human and beast parts"
    );
    eprintln!(
        "installed skin assembly: {races} races, {} authored models, {shapes} shapes",
        paths.len()
    );
}
