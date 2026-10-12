// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-25 player rig and view-switch boundary checks.

use soe_esm::GlobalFormId;
use soe_godot_host::{
    actor::{ActorAssets, ActorError},
    input::{scan, Bindings, InputState},
};

#[test]
fn hst_25_invalid_actor_identity_fails_before_loading_first_person_assets() {
    let mut world = soe_game::World::new(soe_esm::LoadOrder::new());
    let missing = GlobalFormId {
        file: u16::MAX,
        object: u32::MAX,
    };
    assert!(matches!(
        ActorAssets::load_player_first_person(&mut world, missing, &soe_bsa::Vfs::new()),
        Err(ActorError::MissingRaceModel { .. })
    ));
}

#[test]
fn hst_25_toggle_pov_is_a_press_edge_not_a_held_repeat() {
    let bindings = Bindings::default();
    let pressed = InputState::gather(&bindings, &|code| code == scan::F, &|_| false, 0.0, 0.0);
    let held = InputState::gather(
        &bindings,
        &|code| code == scan::F,
        &|code| code == scan::F,
        0.0,
        0.0,
    );
    assert!(pressed.toggle_pov_pressed);
    assert!(!held.toggle_pov_pressed);
}

#[test]
fn hst_25_installed_player_has_a_separate_first_person_rig() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let data = std::path::PathBuf::from(data);
    let source = soe_godot_host::data_source::DataSource::open(&data).unwrap();
    let mut vfs = soe_bsa::Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&data.join(archive)).unwrap();
    }
    let mut world = soe_game::World::from_shared_order(source.shared_order());
    let player = world.player();
    let body = ActorAssets::load_for_actor(&mut world, player, &vfs).unwrap();
    let arms = ActorAssets::load_player_first_person(&mut world, player, &vfs).unwrap();
    assert!(!body.skeleton.bones.is_empty() && !body.surfaces.is_empty());
    assert!(!arms.skeleton.bones.is_empty() && !arms.surfaces.is_empty());
    assert!(arms.skeleton_path.contains("_1stperson"));
}
