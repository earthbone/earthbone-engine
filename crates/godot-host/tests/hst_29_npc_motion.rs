// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-29 uses the AI-90 Patrol fixture and the host's actual placement sampler.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::world::RefPlacement;
use soe_game::Game;
use soe_godot_host::actor::{npc_transform, NpcMotion};
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x803,
};

fn patrol() -> Game {
    let template = package::close_header(package::build_template(0x800, "WalkingPatrol"), None)
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(
            T(b"PTDA"),
            [
                0i32.to_le_bytes(),
                0x804u32.to_le_bytes(),
                0i32.to_le_bytes(),
            ]
            .concat(),
        )
        .sub(T(b"UNAM"), [7]);
    let template = package::with_float_input(template, 11, 0.0);
    let template = package::with_bool_input(template, 3, false);
    let template = package::with_bool_input(template, 9, false);
    let template = package::with_bool_input(template, 14, false);
    let template = package::with_bool_input(template, 15, false);
    let template = package::with_procedure(
        package::finish(template),
        "Patrol",
        &[7, 11, 3, 9, 14, 15],
        false,
    );
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template);
    plugin.push_top(package::finish(package::close_header(
        package::build_record(0x801, "Patrol", 0, Schedule::default()),
        Some(0x800),
    )));
    plugin.push_top(
        npc::build_record(
            0x802,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), 0x801u32.to_le_bytes()),
    );
    let mut actor_data = vec![0; 24];
    actor_data[20..24].copy_from_slice(&std::f32::consts::FRAC_PI_2.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR.object)
            .sub(T(b"NAME"), 0x802u32.to_le_bytes())
            .sub(T(b"DATA"), actor_data),
    );
    for (form, x, next) in [(0x804, 110.0f32, Some(0x805u32)), (0x805, 330.0, None)] {
        let mut data = vec![0; 24];
        data[..4].copy_from_slice(&x.to_le_bytes());
        let mut marker = Record::new(T(b"REFR"), form)
            .sub(T(b"NAME"), 0u32.to_le_bytes())
            .sub(T(b"DATA"), data)
            .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
            .sub(T(b"XPPA"), [])
            .sub(T(b"INAM"), 0u32.to_le_bytes());
        if let Some(next) = next {
            marker = marker.sub(T(b"XLKR"), next.to_le_bytes());
        }
        plugin.push_top(marker);
    }
    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(ACTOR));
    game
}

#[test]
fn hst_29_patrol_host_transform_follows_game_within_eight_units_each_frame() {
    let mut game = patrol();
    let mut previous = game.world().ref_placement(ACTOR).unwrap();
    let mut moving = 0;
    let mut stopped = 0;
    for _ in 0..600 {
        game.tick(1.0 / 60.0);
        let placement = game.world().ref_placement(ACTOR).unwrap();
        let host = npc_transform(placement);
        let position = host.origin.to_skyrim();
        let error = [position.x, position.y, position.z]
            .into_iter()
            .zip(placement.position)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!(
            error <= 8.0,
            "host transform trails Patrol by {error} units"
        );
        let forward = [host.basis[0][2], host.basis[1][2], host.basis[2][2]];
        assert!(forward[0] < -0.99, "heading is carried into the host basis");
        let motion = NpcMotion::between(previous, placement, 1.0 / 60.0).unwrap();
        if motion.speed > 0.1 {
            moving += 1;
            assert!(
                motion.direction.abs() < 0.01,
                "heading-relative walk direction"
            );
        } else {
            stopped += 1;
        }
        previous = placement;
    }
    assert!(
        (previous.position[0] - 330.0).abs() < 0.01,
        "Patrol reaches its final marker"
    );
    assert!(moving > 60 && stopped > 60, "Patrol walks and dwells");
}

fn placement(position: [f32; 3], heading: f32) -> RefPlacement {
    RefPlacement {
        position,
        rotation: [0.0, 0.0, heading],
        scale: 1.0,
        visible: true,
    }
}

#[test]
fn hst_29_motion_uses_frame_delta_and_wraps_heading() {
    let before = placement([0.0; 3], 179.0f32.to_radians());
    let after = placement([0.0, -10.0, 0.0], (-179.0f32).to_radians());
    let motion = NpcMotion::between(before, after, 0.1).unwrap();
    assert!((motion.speed - 100.0).abs() < 0.001);
    assert!((motion.direction + 1.0).abs() < 0.001);
    assert!((motion.turn - 20.0).abs() < 0.001);
    assert_eq!(NpcMotion::between(after, after, 0.1).unwrap().speed, 0.0);
}

#[test]
fn hst_29_motion_rejects_invalid_frame_deltas() {
    let before = placement([0.0; 3], 0.0);
    for delta in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(NpcMotion::between(before, before, delta).is_none());
    }
}

#[cfg(unix)]
fn prepare_extension(root: &std::path::Path) {
    let configuration = std::fs::read_to_string(root.join("godot/soe.gdextension")).unwrap();
    let alias = root.join("target/hst_23_previous_libsoe_godot_host.so");
    // CONTEXT: This base names an HST-23 scratch library. Keep its config intact.
    if configuration.contains("res://../target/hst_23_previous_libsoe_godot_host.so")
        && std::fs::symlink_metadata(&alias).is_err()
    {
        std::os::unix::fs::symlink("release/libsoe_godot_host.so", alias).unwrap();
    }
}

fn capture(mut stream: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        bytes
    })
}

#[test]
fn hst_29_installed_walkthrough_npc_pose_and_moves_gate() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    #[cfg(unix)]
    prepare_extension(&root);
    let mut child = std::process::Command::new(binary)
        .args(["--path"])
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "hst_29_walkthrough_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_PLAY_PATH", "gate")
        .env("SOE_GODOT_TIMEOUT", "360")
        .env("SOE_THREADED_SIMULATION", "0")
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = capture(child.stdout.take().unwrap());
    let stderr = capture(child.stderr.take().unwrap());
    let started = std::time::Instant::now();
    while child.try_wait().unwrap().is_none() {
        if started.elapsed() > std::time::Duration::from_mins(15) {
            child.kill().unwrap();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let output = std::process::Output {
        status: child.wait().unwrap(),
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    };
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(root.join("godot/.godot/hst_29_walkthrough.log"), &log).unwrap();
    for line in log.lines().filter(|line| {
        line.starts_with("PLAY_PATH")
            || line.starts_with("RESULT:")
            || line.starts_with("FAIL")
            || line.starts_with("SCRIPT ERROR:")
    }) {
        println!("{line}");
    }
    assert!(
        output.status.success() && !log.contains("SCRIPT ERROR:"),
        "Godot exited {}\n{log}",
        output.status
    );
    for marker in [
        "PLAY_PATH npc.pose pass",
        "PLAY_PATH npc.moves pass",
        "RESULT: PASS hst_29",
    ] {
        assert!(log.contains(marker), "missing {marker}\n{log}");
    }
}

#[test]
fn hst_29_installed_rig_walk_animates_legs_and_stop_returns_to_idle() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let data = std::path::Path::new(&data);
    let loadout = soe_profile::LoadoutInput::vanilla(data.parent().unwrap(), None, None)
        .resolve()
        .unwrap();
    let source = soe_godot_host::data_source::DataSource::open_loadout(&loadout).unwrap();
    let vfs = soe_godot_host::data_source::vfs_from_loadout(&loadout).unwrap();
    let mut game = source.localized_game().unwrap();
    let cell = game.form("WhiterunBanneredMare").unwrap();
    game.change_cell(Some(cell));
    let contacts = game.world_mut().contact_actors();
    let actor = contacts
        .into_iter()
        .find(|contact| game.world().name_of(contact.actor).as_deref() == Some("Hulda"))
        .unwrap()
        .actor;
    let female = game.world_mut().actor_appearance(actor).unwrap().female;
    let assets =
        soe_godot_host::actor::ActorAssets::load_for_actor(&mut game.world_mut(), actor, &vfs)
            .unwrap();
    let mut vm = assets.npc_interpreter(female);
    let legs: Vec<_> = assets
        .skeleton
        .bones
        .iter()
        .enumerate()
        .filter(|(_, bone)| {
            ["Thigh", "Calf", "Foot"]
                .iter()
                .any(|name| bone.name.contains(name))
        })
        .map(|(index, _)| index)
        .collect();
    assert!(legs.len() >= 6, "vanilla rig supplies both legs");
    let has_clip = |vm: &soe_hkx::interp::Interpreter<'_>, text: &str| {
        vm.active_clips().iter().any(|(id, _)| {
            matches!(&assets.graph.node(*id).kind, soe_hkx::behavior::NodeKind::Clip(clip)
            if clip.animation_name.to_ascii_lowercase().contains(text))
        })
    };
    for _ in 0..60 {
        vm.update(1.0 / 60.0);
    }
    println!(
        "HST-29 rig graph_nodes={} clips={} missing={} states={:?} active={:?}",
        assets.graph.nodes.len(),
        assets.clips.len(),
        assets.missing_clips.len(),
        vm.active_states()
            .iter()
            .map(|(id, state)| (&assets.graph.node(*id).name, state))
            .collect::<Vec<_>>(),
        vm.active_clips()
    );
    assert!(has_clip(&vm, "idle"));
    let idle = vm.pose().to_vec();
    for _ in 0..60 {
        vm.update(1.0 / 60.0);
    }
    assert_ne!(idle, vm.pose(), "authored idle changes pose");
    let motion = NpcMotion::between(
        placement([0.0; 3], 0.0),
        placement([0.0, 100.0, 0.0], 0.0),
        1.0,
    )
    .unwrap();
    vm.variables.set_by_name("Speed", motion.speed);
    vm.variables.set_by_name("Direction", motion.direction);
    assert!(vm.send_event_named("moveStart"));
    let mut changed_frames = 0;
    let mut before = vm.pose().to_vec();
    for _ in 0..120 {
        vm.update(1.0 / 60.0);
        if legs.iter().any(|index| before[*index] != vm.pose()[*index]) {
            changed_frames += 1;
        }
        before.clone_from_slice(vm.pose());
    }
    assert!(
        changed_frames >= 90,
        "walking must animate legs on at least 90 of 120 frames, got {changed_frames}"
    );
    assert!(has_clip(&vm, "walk"), "motion selects authored walk clips");
    let stopped =
        NpcMotion::between(placement([0.0; 3], 0.0), placement([0.0; 3], 0.0), 1.0).unwrap();
    vm.variables.set_by_name("Speed", stopped.speed);
    assert!(vm.send_event_named("moveStop"));
    for _ in 0..60 {
        vm.update(1.0 / 60.0);
    }
    assert!(has_clip(&vm, "idle"), "stopping returns to authored idle");
}
