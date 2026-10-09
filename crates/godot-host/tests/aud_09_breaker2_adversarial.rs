// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn aud_09_player_cell_transition_releases_only_departed_ambience() {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from);
    let godot = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from);
    let (Some(data), Some(godot)) = (data, godot) else {
        panic!("SKYRIM_DATA and SOE_GODOT_BIN are required for the PlayerNode check");
    };

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    assert!(extension.exists(), "build the release GDExtension first");

    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("aud_09_player_transition.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree
var failures := 0
func check(value: bool, message: String) -> void:
    if not value:
        failures += 1
        printerr("FAIL: ", message)
func _initialize() -> void:
    call_deferred("run")
func run() -> void:
    var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
    root.add_child(streamer)
    check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "game data loads")
    var player: Node3D = ClassDB.instantiate("PlayerNode")
    player.set("capture_mouse", false)
    player.set("auto_step", false)
    root.add_child(player)
    check(player.call("use_streamer_data", streamer), "PlayerNode shares game data")
    check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
        "source cell loads")
    check(player.call("start_gameplay"), "gameplay starts")
    var bank := player.get_node_or_null("SoundBank")
    check(bank != null, "SoundBank attaches")
    if bank == null:
        quit(1)
        return
    var starts := int(bank.call("ambience_start_count"))
    check(starts > 0, "source cell has eligible ambience")
    await process_frame
    var departed: Array[AudioStreamPlayer] = []
    for child in bank.get_children():
        if child is AudioStreamPlayer and child.playing:
            departed.append(child)
    check(not departed.is_empty(), "source descriptors are playing")

    check(player.call("enter_cell", "WhiterunBanneredMare", 4.0, 0.0, 500.0) > 0,
        "same cell can be repositioned")
    await process_frame
    check(int(bank.call("ambience_start_count")) == starts,
        "same-cell reposition does not duplicate cell ambience")
    check(player.call("enter_cell", "AUD09MissingCell", 0.0, 0.0, 0.0) < 0,
        "missing destination is rejected")
    await process_frame
    for old_player in departed:
        check(is_instance_valid(old_player) and old_player.playing,
            "failed destination leaves current ambience playing")

    var destination_started := false
    for destination in ["WhiterunDragonsreach", "WhiterunJorrvaskr",
            "WhiterunTempleofKynareth", "WhiterunHallofTheDead",
            "WhiterunDrunkenHuntsman", "WhiterunArcadiasCauldron",
            "WhiterunWarmaidens", "WhiterunBreezehome"]:
        if player.call("enter_cell", destination, 0.0, 0.0, 500.0) < 0:
            continue
        await process_frame
        if int(bank.call("ambience_start_count")) > starts:
            destination_started = true
            break
    check(destination_started, "no tested destination cell started eligible ambience")
    for old_player in departed:
        check(not is_instance_valid(old_player) or not old_player.playing,
            "departed descriptor stops after destination transition")
    player.queue_free()
    streamer.queue_free()
    await process_frame
    print("AUD-09 ADVERSARIAL RESULT: ", "PASS" if failures == 0 else "FAIL")
    quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--windowed", "--resolution", "640x360", "--script"])
        .arg(script)
        .env("SKYRIM_DATA", data)
        .output()
        .expect("Godot PlayerNode ambience process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-09 ADVERSARIAL RESULT: PASS"),
        "PlayerNode ambience adversarial check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
