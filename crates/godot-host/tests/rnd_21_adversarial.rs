// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Exercise RND-21's exposed lighting resolver through a live Godot process.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn installed_cell_lighting_distinguishes_interior_exterior_and_missing_cells() {
    let (Ok(data), Ok(godot)) = (std::env::var("SKYRIM_DATA"), std::env::var("SOE_GODOT_BIN"))
    else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN not set, skipping the cell lighting probe");
        return;
    };
    let probe_name = format!("rnd_21_probe_{}.gd", std::process::id());
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot");
    let probe_path = project.join(&probe_name);
    let probe = r#"extends SceneTree

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	var streamer: Node = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	if not streamer.call("load_data", data):
		push_error("installed data failed to load")
		quit(1)
		return
	var interior: Dictionary = streamer.call("cell_lighting", "WhiterunBanneredMare")
	for key in ["ambient", "directional", "ambient_directions", "fog_near_color", "fog_far_color", "fog_near", "fog_far", "rotation_xy", "rotation_z", "directional_fade", "fog_clip_distance", "fog_power", "fog_max", "light_fade_begin", "light_fade_end"]:
		if not interior.has(key):
			push_error("interior lighting is missing " + key)
			quit(1)
			return
	if not interior["ambient"] is Color or not interior["directional"] is Color or interior["ambient_directions"].size() != 6:
		push_error("interior colors or six directional ambient values are malformed")
		quit(1)
		return
	if interior != streamer.call("cell_lighting", "WhiterunBanneredMare"):
		push_error("repeated resolution changed the lighting result")
		quit(1)
		return
	if not streamer.call("cell_lighting", "RND21MissingCell").is_empty():
		push_error("an unknown editor ID returned cell lighting")
		quit(1)
		return
	var exterior_name := ""
	for row in streamer.call("editor_cell_tree"):
		if not row["interior"] and row["references"] > 0:
			exterior_name = row["name"]
			break
	if exterior_name.is_empty() or not streamer.call("cell_lighting", exterior_name).is_empty():
		push_error("an installed exterior cell received interior lighting")
		quit(1)
		return
	print("RND21_PROBE_PASS interior=WhiterunBanneredMare exterior=" + exterior_name)
	quit(0)
"#;
    fs::write(&probe_path, probe).expect("write temporary Godot probe");
    let result = Command::new(godot)
        .args([
            "--headless",
            "--path",
            project.to_str().expect("Godot project path is UTF-8"),
            "--script",
            &format!("res://{probe_name}"),
        ])
        .env("SKYRIM_DATA", data)
        .output()
        .expect("run Godot lighting probe");
    let _ = fs::remove_file(&probe_path);
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains("RND21_PROBE_PASS"),
        "Godot lighting probe failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
