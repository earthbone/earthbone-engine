// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Runs the installed-data walkthrough in a window and checks its audio output.

#[path = "common/aud_11.rs"]
mod aud_11;

use std::path::PathBuf;
use std::process::Command;

#[test]
fn aud_11_walkthrough_audio_runs_for_twenty_seconds_and_reports_each_playing_system() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };

    let project = aud_11::project(&["walkthrough.gd", "render_warmup.gd", "system_overlay.gd"]);

    let output = Command::new(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(project.path().join("walkthrough.gd"))
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .env("SOE_AUD_11_TEST", "1")
        .output()
        .expect("Godot walkthrough process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-11 RESULT: PASS"),
        "windowed AUD-11 walkthrough failed with {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        output.status
    );
    for system in [
        "footsteps=",
        "ambience=",
        "music=",
        "voices=",
        "master_peak_db=",
    ] {
        assert!(
            stdout.contains(system),
            "missing {system} in walkthrough report\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
    }
    assert!(
        stdout.contains("walk_seconds=20"),
        "walk duration was not reported\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let report = stdout
        .lines()
        .find(|line| line.contains("AUD-11 systems"))
        .expect("walkthrough emits its system report");
    eprintln!("{report}");
    let walk_distance = report
        .split_once("walk_distance=")
        .and_then(|(_, value)| value.split_whitespace().next())
        .and_then(|value| value.parse::<f32>().ok())
        .expect("walk distance is reported as a number");
    assert!(
        walk_distance > 10.0,
        "walk did not move across the cell: {walk_distance} units"
    );
}

#[test]
fn aud_11_vanilla_nearby_idle_response_resolves_and_decodes_voice_asset() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source = soe_godot_host::data_source::DataSource::open(&data).unwrap();
    let mut game = source.localized_game().unwrap();
    let loadout = soe_profile::LoadoutInput {
        game_dir: data.parent().unwrap().to_path_buf(),
        data_roots: vec![data.clone()],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    }
    .resolve()
    .unwrap();
    let vfs = soe_godot_host::data_source::vfs_from_loadout(&loadout).unwrap();
    game.load_scripts_from_vfs(&vfs);
    game.start();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let player = game.player();
    game.change_cell(Some(cell));
    game.run_until_idle(64);
    let hulda = game.world().form("HuldaRef").unwrap();
    {
        let mut world = game.world_mut();
        assert!(world.drive_actor(hulda));
        let speaker = world.reference(hulda).unwrap();
        let position = speaker.position;
        assert_eq!(speaker.cell, Some(cell));
        let player_ref = world.reference(player).unwrap();
        player_ref.cell = Some(cell);
        player_ref.position = [position[0], position[1] - 100.0, position[2]];
        world.debug_ai_enabled = false;
    }
    game.tick(1.0 / 60.0);
    assert!(
        game.ambient_dialogue(),
        "nearby NPC did not select ambient dialogue: {:?}",
        game.world().dialogue_session()
    );
    let line = game.dialogue_line().unwrap().unwrap();
    let asset = soe_godot_host::dialogue_menu::voice_asset(&mut game, &line)
        .expect("ambient INFO resolves its speaker's voice asset");
    let wma = soe_audio::wma::Native;
    let decoded = soe_audio::SoundSource::new(&vfs)
        .with_wma_decoder(&wma)
        .read(&asset.path)
        .unwrap();
    assert!(
        decoded.pcm.peak() > 0.001,
        "voice asset is silent: {}",
        asset.path
    );
}
