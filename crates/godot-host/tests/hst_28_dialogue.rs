// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed Bannered Mare NPC activation through Godot input and native menus.

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[path = "common/debug_project.rs"]
mod debug_project;

#[test]
fn hst_28_bannered_mare_hulda_activation_greeting_topics_voice_and_close() {
    run_hulda("input");
}

#[test]
fn hst_28_play_path_activation_selects_topic_response_and_starts_voice() {
    run_hulda("play_path");
}

fn run_hulda(activation: &str) {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let cell = game.form("WhiterunBanneredMare").unwrap();
    game.change_cell(Some(cell));
    let contacts = game.world_mut().contact_actors();
    let hulda = contacts
        .iter()
        .find(|actor| game.world().name_of(actor.actor).as_deref() == Some("Hulda"))
        .unwrap()
        .actor;
    let player = game.player();
    let order = source.shared_order();
    let music = order
        .winners_of(soe_esm::Tag(*b"MUSC"))
        .find_map(|(id, _)| {
            [0.0, 0.25, 0.5, 0.75, 1.0].into_iter().find_map(|draw| {
                soe_game::music::select(&mut game, id, player, draw)
                    .ok()
                    .and_then(|track| track.path)
            })
        })
        .expect("an audible installed music track");
    drop(order);
    drop(game);
    drop(source);
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN for host checks");
    let output = std::process::Command::new(binary)
        .args(["--headless", "--path"])
        .arg(root.join("godot"))
        .arg("--script")
        .arg(root.join("crates/godot-host/tests/hst_28_dialogue.gd"))
        .env("SOE_THREADED_SIMULATION", "0")
        .env("SOE_HST_28_SPEAKER_FILE", hulda.file.to_string())
        .env("SOE_HST_28_SPEAKER_OBJECT", hulda.object.to_string())
        .env("SOE_HST_28_ACTIVATION", activation)
        .env("SOE_HST_28_MUSIC_PATH", music)
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(root.join("target/hst-28-scene.log"), &log).unwrap();
    for line in log.lines().filter(|line| {
        line.contains("hst_28") || line.contains("SCRIPT ERROR:") || line.starts_with("PLAY_PATH ")
    }) {
        println!("{line}");
    }
    assert!(
        output.status.success(),
        "Godot exit {:?}",
        output.status.code()
    );
    assert!(!log.contains("SCRIPT ERROR:"), "GDScript failed");
    assert!(log.contains("PASS hst_28"), "host check did not finish");
}

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn freewalk_dialogue_controls_fit_small_portrait_and_ultrawide_viewports() {
    let Some(binary) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let project = debug_project::project(&["freewalk_menu_layout_test.gd"]).unwrap();
    let output = std::process::Command::new(binary)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "freewalk_menu_layout_test.gd"])
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::write(root.join("target/freewalk-menu-layout.log"), &log).unwrap();
    assert!(
        output.status.success()
            && log.contains("PASS freewalk_menu_layout")
            && !log.contains("SCRIPT ERROR:"),
        "{log}"
    );
}
