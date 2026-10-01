// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_esm::{LoadOrder, Plugin, Tag};
use soe_game::appearance::ActorAppearance;
use soe_game::race::AnimationModels;
use soe_godot_host::actor::ActorAssets;

#[test]
fn behavior_model_is_read_only_from_the_nam3_behavior_section() {
    let record = soe_esm::Record::new(Tag::new(b"RACE"), 0x100)
        .sub(Tag::new(b"MNAM"), vec![])
        .zsub(Tag::new(b"ANAM"), r"Actors\Human\skeleton.nif")
        .zsub(Tag::new(b"MODL"), r"Actors\Human\body-model.nif")
        .sub(Tag::new(b"NAM3"), vec![])
        .sub(Tag::new(b"MNAM"), vec![])
        .zsub(Tag::new(b"MODL"), r"Actors\Human\Behaviors\male.hkx");

    let models = AnimationModels::from_record(&record).expect("RACE models parse");
    let selected = models.select(false);

    assert_eq!(
        selected.skeleton.as_deref(),
        Some(r"Actors\Human\skeleton.nif")
    );
    assert_eq!(
        selected.behavior.as_deref(),
        Some(r"Actors\Human\Behaviors\male.hkx")
    );
}

#[test]
fn a_pre_nam3_model_does_not_fill_an_absent_behavior_graph() {
    let record = soe_esm::Record::new(Tag::new(b"RACE"), 0x100)
        .sub(Tag::new(b"MNAM"), vec![])
        .zsub(Tag::new(b"ANAM"), r"Actors\Creature\skeleton.nif")
        .zsub(Tag::new(b"MODL"), r"Actors\Creature\unrelated.nif");

    let models = AnimationModels::from_record(&record).expect("RACE models parse");

    assert_eq!(models.select(false).behavior, None);
}

#[test]
fn installed_human_beast_and_creature_race_rigs_map_all_skin_bones() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut vfs = Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&data.join(name))
            .unwrap_or_else(|error| panic!("open {name}: {error}"));
    }
    let mut order = LoadOrder::new();
    order
        .add(
            "Skyrim.esm",
            Plugin::parse(&std::fs::read(data.join("Skyrim.esm")).expect("read Skyrim.esm"))
                .expect("parse Skyrim.esm"),
        )
        .expect("load Skyrim.esm");

    for expected in ["NordRace", "ArgonianRace", "DraugrRace"] {
        let (race_id, race) = order
            .winners_of(Tag::new(b"RACE"))
            .find(|(_, record)| record.editor_id().as_deref() == Some(expected))
            .unwrap_or_else(|| panic!("Skyrim.esm has {expected}"));
        let selected = AnimationModels::from_record(race)
            .expect("parse race models")
            .select(false);
        let skeleton = selected
            .skeleton
            .as_deref()
            .unwrap_or_else(|| panic!("{expected} has a male skeleton"));
        let behavior = selected
            .behavior
            .as_deref()
            .unwrap_or_else(|| panic!("{expected} has a male behavior graph"));
        let appearance = ActorAppearance::from_order(&order, None, Some(race_id), &BTreeSet::new())
            .unwrap_or_else(|error| panic!("{expected} appearance: {error}"));
        let assets = ActorAssets::load_for_race(&vfs, race, false, &appearance)
            .unwrap_or_else(|error| panic!("{expected} rig: {error}"));

        assert!(
            !assets.skeleton.bones.is_empty(),
            "{expected} skeleton loads"
        );
        assert!(
            !assets.graph.nodes.is_empty(),
            "{expected} behavior graph loads"
        );
        assert!(
            assets
                .skeleton_model_path
                .to_ascii_lowercase()
                .ends_with(&skeleton.replace('\\', "/").to_ascii_lowercase()),
            "{expected} selected authored skeleton is loaded"
        );
        assert!(
            assets
                .project_path
                .as_deref()
                .unwrap()
                .to_ascii_lowercase()
                .ends_with(&behavior.replace('\\', "/").to_ascii_lowercase()),
            "{expected} selected authored behavior graph is loaded"
        );
        for surface in &assets.surfaces {
            assert!(
                surface.map.unmatched.is_empty(),
                "{expected} skin {} has unmapped bones {:?}",
                surface.name,
                surface.map.unmatched
            );
        }
    }
}
