// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Tests against Skyrim SE's own files. They need `hkx` files extracted
//! from `Skyrim - Animations.bsa` in a directory named by `SOE_HKX_DIR`
//! (default `engine-refs/skyrim-hkx-fixtures` next to the engine checkout)
//! and the Nemesis SSE cached behaviors under `engine-refs`. Each test
//! skips with a message when its files are absent.

use std::path::{Path, PathBuf};

use soe_havok::animation::Animation;
use soe_havok::behavior::{BehaviorGraph, NodeKind};
use soe_havok::character::CharacterData;
use soe_havok::packfile::root_variants;
use soe_havok::skeleton::AnimationContainer;
use soe_havok::Packfile;

fn engine_refs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine-refs")
}

fn fixture(rel: &str) -> Option<Vec<u8>> {
    let dir = std::env::var_os("SOE_HKX_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| engine_refs().join("skyrim-hkx-fixtures"));
    let path = dir.join(rel);
    match std::fs::read(&path) {
        Ok(b) => Some(b),
        Err(_) => {
            eprintln!("skipping: {} not present", path.display());
            None
        }
    }
}

#[test]
fn the_player_skeleton_loads_with_its_bone_tree() {
    let Some(bytes) = fixture("meshes/actors/character/character assets/skeleton.hkx") else {
        return;
    };
    let pf = Packfile::parse(&bytes).unwrap();
    assert_eq!(pf.header.version_string, "hk_2010.2.0-r1");
    let c = AnimationContainer::from_packfile(&pf).unwrap();
    assert!(!c.skeletons.is_empty());
    let sk = &c.skeletons[0];
    eprintln!(
        "skeleton {:?}: {} bones, {} float slots, {} skeletons in file",
        sk.name,
        sk.bones.len(),
        sk.float_slots.len(),
        c.skeletons.len()
    );
    assert!(sk.bones.len() > 50, "{} bones", sk.bones.len());
    assert_eq!(sk.bones[0].name, "NPC Root [Root]");
    assert_eq!(sk.parent_indices[0], -1);
    assert!(sk.bone_index("NPC Head [Head]").is_some());
    let model = sk.to_model_space(&sk.reference_pose).unwrap();
    assert_eq!(model.len(), sk.bones.len());
    let head = model[sk.bone_index("NPC Head [Head]").unwrap()];
    assert!(
        head.translation[2] > 100.0,
        "head at {:?}",
        head.translation
    );
}

#[test]
fn a_vanilla_animation_decodes_every_block_to_unit_quaternions() {
    let Some(bytes) = fixture("meshes/actors/character/animations/mt_jump.hkx") else {
        return;
    };
    let pf = Packfile::parse(&bytes).unwrap();
    let c = AnimationContainer::from_packfile(&pf).unwrap();
    assert_eq!(c.animations.len(), 1);
    let Animation::SplineCompressed(a) = &c.animations[0] else {
        panic!("{:?}", c.animations[0]);
    };
    eprintln!(
        "mt_jump: {:.3}s, {} frames, {} blocks, {} transform tracks, {} float tracks, {} annotation tracks",
        a.duration,
        a.num_frames,
        a.num_blocks,
        a.num_transform_tracks,
        a.num_float_tracks,
        a.annotation_tracks.len()
    );
    let blocks = a.decode().unwrap();
    assert_eq!(blocks.len(), a.num_blocks);
    for b in &blocks {
        assert_eq!(b.tracks.len(), a.num_transform_tracks);
    }
    // An empty track-to-bone map means the identity mapping.
    let binding = &c.bindings[0];
    assert!(
        binding.transform_track_to_bone_indices.is_empty()
            || binding.transform_track_to_bone_indices.len() == a.num_transform_tracks
    );
    let steps = 10;
    for i in 0..=steps {
        let t = a.duration * i as f32 / steps as f32;
        let pose = a.sample_pose(&blocks, t);
        assert_eq!(pose.len(), a.num_transform_tracks);
        for (track, p) in pose.iter().enumerate() {
            let len: f32 = p.rotation.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!(
                (len - 1.0).abs() < 2e-2,
                "t={t} track {track}: rotation {:?} has length {len}",
                p.rotation
            );
            assert!(p.translation.iter().all(|x| x.is_finite()));
            assert!(p.scale.iter().all(|x| x.is_finite()));
        }
    }
}

#[test]
fn the_master_behavior_loads_as_a_graph() {
    let Some(bytes) = fixture("meshes/actors/character/behaviors/0_master.hkx") else {
        return;
    };
    let pf = Packfile::parse(&bytes).unwrap();
    let g = BehaviorGraph::from_packfile(&pf).unwrap();
    let clips = g.nodes_of_class("hkbClipGenerator").count();
    let machines = g.nodes_of_class("hkbStateMachine").count();
    let unknown = g
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Unknown))
        .count();
    let mut unknown_classes: Vec<&str> = g
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Unknown))
        .map(|n| n.class.as_str())
        .collect();
    unknown_classes.sort_unstable();
    unknown_classes.dedup();
    eprintln!(
        "0_master: {} nodes ({} state machines, {} clips, {} unknown: {:?}), {} events, {} variables, {} referenced behaviors",
        g.nodes.len(),
        machines,
        clips,
        unknown,
        unknown_classes,
        g.data.event_names.len(),
        g.data.variable_names.len(),
        g.referenced_behaviors().len()
    );
    assert!(g.nodes.len() > 100);
    assert!(machines > 0);
    assert!(!g.data.event_names.is_empty());
    assert!(!g.data.variable_names.is_empty());
    assert_eq!(g.data.variable_infos.len(), g.data.variable_names.len());
    assert_eq!(g.data.event_infos.len(), g.data.event_names.len());
    let root = g.node(g.root.unwrap());
    assert!(
        matches!(root.kind, NodeKind::StateMachine(_)),
        "{}",
        root.class
    );
    assert!(!g.referenced_behaviors().is_empty());
    assert!(
        g.data.variable_index("iState").is_some(),
        "{:?}",
        &g.data.variable_names[..10]
    );
}

#[test]
fn every_nemesis_cached_behavior_loads() {
    let dir = engine_refs()
        .join("Project-New-Reign---Nemesis-Main/test environment/cached_behaviors/SSE");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipping: {} not present", dir.display());
        return;
    };
    // The directory mixes behavior files with the two character files
    // (defaultmale, defaultfemale), so dispatch on the root variant's class.
    let mut behaviors = 0;
    let mut characters = 0;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "hkx") {
            continue;
        }
        let bytes = std::fs::read(&p).unwrap();
        let pf = Packfile::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let classes: Vec<String> = root_variants(&pf)
            .unwrap()
            .into_iter()
            .map(|v| v.class_name)
            .collect();
        if classes.iter().any(|c| c == "hkbBehaviorGraph") {
            let g = BehaviorGraph::from_packfile(&pf)
                .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert!(g.root.is_some(), "{}", p.display());
            behaviors += 1;
        } else if classes.iter().any(|c| c == "hkbCharacterData") {
            CharacterData::from_packfile(&pf).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            characters += 1;
        } else {
            panic!("{}: unexpected root {classes:?}", p.display());
        }
    }
    eprintln!("nemesis: {behaviors} behavior files and {characters} character files loaded");
    assert!(behaviors > 10);
    assert_eq!(characters, 2);
}

#[test]
fn the_default_male_character_names_its_rig_and_behavior() {
    let Some(bytes) = fixture("meshes/actors/character/characters/defaultmale.hkx") else {
        return;
    };
    let pf = Packfile::parse(&bytes).unwrap();
    let c = CharacterData::from_packfile(&pf).unwrap();
    eprintln!(
        "defaultmale: name {:?}, rig {:?}, ragdoll {:?}, behavior {:?}, {} animations, {} character properties",
        c.name,
        c.rig_name,
        c.ragdoll_name,
        c.behavior_filename,
        c.animation_names.len(),
        c.character_property_names.len()
    );
    assert!(c.rig_name.to_lowercase().contains("skeleton"));
    assert!(c.behavior_filename.to_lowercase().contains("0_master"));
    assert!(!c.animation_names.is_empty());
    assert_eq!(
        c.character_property_infos.len(),
        c.character_property_names.len()
    );
}
