// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use soe_bsa::Vfs;
use soe_esm::{LoadOrder, Plugin, Tag};
use soe_game::appearance::ActorAppearance;
use soe_game::race::AnimationModels;
use soe_godot_host::actor::ActorAssets;

fn installed_data() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn character_vfs(data: &std::path::Path) -> Option<Vfs> {
    let mut vfs = Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        let path = data.join(name);
        if !path.exists() {
            eprintln!("skipping: {} is not there", path.display());
            return None;
        }
        vfs.add_archive_path(&path).expect("archive indexes");
    }
    Some(vfs)
}

#[test]
fn anm_55_installed_human_beast_and_creature_races_load_their_selected_rigs() {
    let Some(data) = installed_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let Some(vfs) = character_vfs(&data) else {
        return;
    };
    let mut order = LoadOrder::new();
    order
        .add(
            "Skyrim.esm",
            Plugin::parse(&std::fs::read(data.join("Skyrim.esm")).expect("Skyrim.esm"))
                .expect("Skyrim.esm parses"),
        )
        .expect("load order accepts Skyrim.esm");

    let targets = ["NordRace", "ArgonianRace", "DraugrRace"];
    for expected in targets {
        let (race_id, race) = order
            .winners_of(Tag::new(b"RACE"))
            .find(|(_, record)| record.editor_id().as_deref() == Some(expected))
            .unwrap_or_else(|| panic!("installed Skyrim.esm has {expected}"));
        let selected = AnimationModels::from_record(race)
            .expect("race animation models parse")
            .select(false);
        let skeleton = selected
            .skeleton
            .as_deref()
            .unwrap_or_else(|| panic!("{expected} has a male skeletal model"));
        let behavior = selected
            .behavior
            .as_deref()
            .unwrap_or_else(|| panic!("{expected} has a male behavior graph"));
        let appearance = ActorAppearance::from_order(&order, None, Some(race_id), &BTreeSet::new())
            .unwrap_or_else(|error| panic!("{expected} appearance: {error}"));
        let assets = ActorAssets::load_for_race(&vfs, race, false, &appearance)
            .unwrap_or_else(|error| panic!("{expected} rig {skeleton} + {behavior}: {error}"));

        assert!(
            !assets.skeleton.bones.is_empty(),
            "{expected} skeleton loads"
        );
        assert!(!assets.graph.nodes.is_empty(), "{expected} graph loads");
        let selected_skeleton = skeleton.replace('\\', "/").to_ascii_lowercase();
        assert!(assets
            .skeleton_model_path
            .to_ascii_lowercase()
            .ends_with(&selected_skeleton));
        assert!(assets
            .project_path
            .as_deref()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with(&behavior.replace('\\', "/").to_ascii_lowercase()));
        for surface in &assets.surfaces {
            assert!(
                surface.map.unmatched.is_empty(),
                "{expected} skin shape {} has unmapped bones {:?}",
                surface.name,
                surface.map.unmatched
            );
        }
    }
}
