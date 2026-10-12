// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed player assets and the walkthrough presentation contract.

use soe_esm::GlobalFormId;
use soe_godot_host::{actor::ActorAssets, data_source::DataSource};
use std::path::PathBuf;

#[test]
fn hst_25_installed_player_has_a_record_selected_body_and_rig() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let data = PathBuf::from(data);
    let source = DataSource::open(&data).unwrap();
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
    assert_eq!(
        player,
        GlobalFormId {
            file: 0,
            object: 0x14
        }
    );
    let assets = ActorAssets::load_for_actor(&mut world, player, &vfs).unwrap();
    assert!(!assets.skeleton.bones.is_empty());
    assert!(!assets.surfaces.is_empty());
    let first = ActorAssets::load_player_first_person(&mut world, player, &vfs).unwrap();
    assert!(!first.surfaces.is_empty());
    assert!(!first.skeleton.bones.is_empty());
    let mut results = Vec::new();
    for (rig, first_person) in [(&assets, false), (&first, true)] {
        let mut vm = soe_hkx::interp::Interpreter::new(
            &rig.graph,
            &rig.clips,
            rig.skeleton.reference_pose.clone(),
        );
        vm.character_properties
            .set_by_name("IsThirdPerson", f32::from(u8::from(!first_person)));
        for (name, value) in [
            ("IsPlayer", 1.0),
            ("iIsPlayer", 1.0),
            ("IsNPC", 0.0),
            ("IsFirstPerson", f32::from(u8::from(first_person))),
            ("i1stPerson", f32::from(u8::from(!first_person))),
            ("iRightHandType", 1.0),
            ("iRightHandEquipped", 1.0),
            ("IsAttackReady", 1.0),
        ] {
            vm.variables.set_by_name(name, value);
        }
        vm.update(0.0);
        assert!(vm.send_event_named("WeapEquip"));
        let mut draw = false;
        for _ in 0..90 {
            vm.update(1.0 / 60.0);
            draw |= vm.active_clips().iter().any(|(id, time)| {
                rig.graph
                    .node(*id)
                    .name
                    .to_ascii_lowercase()
                    .contains("equip")
                    && *time > 0.1
            });
            vm.take_events();
        }
        assert!(vm.send_event_named("attackStart"));
        let before = vm.pose().to_vec();
        let mut attack = false;
        let mut contact = false;
        let mut moved = false;
        for _ in 0..120 {
            vm.update(1.0 / 60.0);
            attack |= vm.active_clips().iter().any(|(id, _)| {
                rig.graph
                    .node(*id)
                    .name
                    .to_ascii_lowercase()
                    .contains("attack")
            });
            moved |= attack && vm.pose() != before.as_slice();
            contact |= vm.take_events().iter().any(|event| {
                usize::try_from(event.id)
                    .ok()
                    .and_then(|id| rig.graph.data.event_names.get(id))
                    .is_some_and(|event| event == "HitFrame")
            });
        }
        results.push((first_person, draw, attack, moved, contact));
    }
    assert!(
        results
            .iter()
            .all(|(_, draw, attack, moved, contact)| *draw && *attack && *moved && *contact),
        "authored (first_person, draw, attack, moved, contact): {results:?}"
    );
    for event in [
        "weaponDraw",
        "weaponSheathe",
        "attackStart",
        "attackStartLeftHand",
        "attackPowerStartInPlace",
        "MLh_SpellAimedConcentrationStart",
        "MRh_SpellRelease_Event",
    ] {
        assert!(
            assets
                .graph
                .data
                .event_names
                .iter()
                .any(|name| name == event),
            "missing player event {event}"
        );
    }
}

#[test]
fn hst_25_walkthrough_binds_both_views_and_captures_the_body() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    let mut failures = Vec::new();
    for headless in [true, false] {
        let mut command = std::process::Command::new(&godot);
        if headless {
            command.arg("--headless");
        }
        let output = command
            .arg("--path")
            .arg(&project)
            .args([
                "--resolution",
                "640x360",
                "--script",
                "hst_25_player_test.gd",
            ])
            .env("SKYRIM_DATA", &data)
            .env_remove("SOE_CAPTURE_PATH")
            .env_remove("SOE_THREADED_SIMULATION")
            .output()
            .expect("Godot starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mode = if headless { "headless" } else { "windowed" };
        std::fs::write(
            project.join(format!(".godot/hst_25_{mode}.log")),
            format!("{stdout}\n{stderr}"),
        )
        .unwrap();
        if !output.status.success() || !stdout.contains("RESULT: PASS HST-25 walkthrough body") {
            let summary = stdout
                .lines()
                .chain(stderr.lines())
                .filter(|line| line.contains("HST-25") || line.contains("SCRIPT ERROR"))
                .collect::<Vec<_>>()
                .join("\n");
            failures.push(format!("{mode} {}: {summary}", output.status));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn hst_25_ready_weapon_uses_the_vanilla_r_press_edge() {
    use soe_godot_host::input::{scan, Bindings, InputState};
    let bindings = Bindings::default();
    let press = InputState::gather(&bindings, &|code| code == scan::R, &|_| false, 0.0, 0.0);
    let hold = InputState::gather(
        &bindings,
        &|code| code == scan::R,
        &|code| code == scan::R,
        0.0,
        0.0,
    );
    assert!(press.ready_weapon_pressed);
    assert!(!hold.ready_weapon_pressed);
}

#[test]
fn hst_25_third_person_boom_stays_in_front_of_a_wall() {
    use soe_godot_host::camera::{CameraMode, CameraRig};
    use soe_physics::Vec3;
    let rig = CameraRig::default().with_mode(CameraMode::Third);
    let feet = Vec3::ZERO;
    let head = rig.head(feet, 128.0);
    let eye = rig.eye(feet, 128.0, Some(50.0));
    let offset = Vec3::new(eye.position.x, eye.position.y, eye.position.z) - head;
    assert!(offset.length() < 50.0, "boom clipped through the wall");
}

#[test]
fn hst_25_missing_player_race_returns_a_typed_asset_error() {
    use soe_godot_host::actor::ActorError;
    let mut world = soe_game::World::new(soe_esm::LoadOrder::new());
    let player = world.player();
    assert!(matches!(
        ActorAssets::load_player_first_person(&mut world, player, &soe_bsa::Vfs::new()),
        Err(ActorError::MissingRaceModel { asset: "race", .. })
    ));
}
