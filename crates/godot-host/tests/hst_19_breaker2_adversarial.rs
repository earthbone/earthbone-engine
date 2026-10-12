// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn hst_19_host_writes_decodable_png_to_unicode_path() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping HST-19 adversarial host check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("SOE_GODOT_BIN unset, skipping HST-19 adversarial host check");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output_path =
        std::env::temp_dir().join(format!("soe-hst19-雪 capture-{}.png", std::process::id()));
    let script_path =
        std::env::temp_dir().join(format!("soe-hst19-雪-capture-{}.gd", std::process::id()));
    let script = r#"extends SceneTree
var failures := 0
func _initialize() -> void:
	call_deferred("_run")
func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", data): failures += 1
	if streamer.call("stream_cell", "WhiterunBanneredMare") <= 0: failures += 1
	if streamer.call("stream_cell_collision", "WhiterunBanneredMare") <= 0: failures += 1
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	root.add_child(player)
	if not player.call("use_streamer_data", streamer): failures += 1
	if player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) <= 0: failures += 1
	if not player.call("start_gameplay"): failures += 1
	await process_frame
	await process_frame
	var path := OS.get_environment("SOE_HST19_OUTPUT")
	if not player.call("take_screenshot", path): failures += 1
	var image := Image.load_from_file(path)
	if image.is_empty() or image.get_width() != 640 or image.get_height() != 360: failures += 1
	player.free()
	streamer.free()
	quit(0 if failures == 0 else 1)
"#;
    std::fs::write(&script_path, script).expect("write temporary Godot check");
    let output = Command::new(godot)
        .args(["--headless", "--path"])
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(&script_path)
        .env("SKYRIM_DATA", data)
        .env("SOE_HST19_OUTPUT", &output_path)
        .output()
        .expect("launch Godot screenshot check");
    let _ = std::fs::remove_file(&script_path);
    let _ = std::fs::remove_file(&output_path);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stderr.contains("SCRIPT ERROR"),
        "HST-19 Unicode path check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
