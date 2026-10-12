// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed Bannered Mare NPC activation through Godot input and native menus.

#[test]
fn hst_28_bannered_mare_hulda_activation_greeting_topics_voice_and_close() {
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
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in log
        .lines()
        .filter(|line| line.contains("hst_28") || line.contains("SCRIPT ERROR:"))
    {
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
