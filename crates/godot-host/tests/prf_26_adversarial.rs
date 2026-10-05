// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-26 transition requests must correspond to successful visible actions.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}

#[test]
fn prf_26_failed_and_repeated_requests_are_not_reported_as_transitions() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").or_else(|| std::env::var_os("SOE_GODOT"))
    else {
        eprintln!("SOE_GODOT_BIN not set, skipping windowed transition check");
        return;
    };
    let root = workspace();
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        root.join(target)
    };
    let library = target.join("debug/deps/libsoe_godot_host.so");
    assert!(library.is_file(), "cargo test must build {}", library.display());

    let project = tempfile::tempdir_in(&root).unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[application]\nconfig/name=\"PRF-26 adversarial\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("soe.gdextension"),
        format!(
            "[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\n",
            library.display()
        ),
    )
    .unwrap();
    std::fs::create_dir(project.path().join(".godot")).unwrap();
    std::fs::write(
        project.path().join(".godot/extension_list.cfg"),
        "res://soe.gdextension\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("prf_26_adversarial.gd"),
        r#"extends SceneTree

var requests: Array[String] = []

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	var scene := Node3D.new()
	root.add_child(scene)
	var player := ClassDB.instantiate("PlayerNode") as Node3D
	scene.add_child(player)
	player.transition_requested.connect(func(kind: String, name: String) -> void:
		requests.append("%s.%s" % [kind, name]))
	await process_frame
	var first_open: bool = player.call("open_menu", "Inventory")
	var duplicate_open: bool = player.call("open_menu", "Magic")
	var first_close: bool = player.call("close_menu")
	var duplicate_close: bool = player.call("close_menu")
	var failed_save: Dictionary = player.call("save_game_generation", "", 1)
	await RenderingServer.frame_post_draw
	await process_frame
	var failures: Array[String] = []
	if not first_open or duplicate_open or not first_close or duplicate_close:
		failures.append("menu return contract changed")
	if failed_save.get("ok", true):
		failures.append("save without gameplay unexpectedly succeeded")
	var expected := ["menu_open.Inventory", "menu_close.Inventory"]
	if requests != expected:
		failures.append("transition requests include failed/replaced actions: " + str(requests))
	print("prf_26_requests=" + str(requests))
	if failures.is_empty():
		print("PRF-26 adversarial checks passed")
		quit(0)
	else:
		for failure in failures:
			printerr("FAIL: " + failure)
		quit(1)
"#,
    )
    .unwrap();

    let output = Command::new(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--resolution", "640x360", "--script", "prf_26_adversarial.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{log}");
    assert!(output.status.success(), "{log}");
    assert!(log.contains("PRF-26 adversarial checks passed"), "{log}");
}
