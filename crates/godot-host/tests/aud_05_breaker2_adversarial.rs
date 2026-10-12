// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{music, Game};
use std::path::PathBuf;
use std::process::Command;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[test]
fn aud_05_palette_fade_out_reaches_selected_leaf() {
    let id = |object| GlobalFormId { file: 0, object };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"MUSC"), 0x800)
            .sub(T(b"FNAM"), 0_u32.to_le_bytes())
            .sub(T(b"PNAM"), [1, 0, 0, 0])
            .sub(T(b"WNAM"), 1.5_f32.to_le_bytes())
            .sub(T(b"TNAM"), 0x801_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), 0x801)
            .sub(T(b"CNAM"), 0x23F6_78C3_u32.to_le_bytes())
            .sub(T(b"DNAM"), 3.25_f32.to_le_bytes())
            .sub(T(b"SNAM"), 0x802_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"MUST"), 0x802)
            .sub(T(b"CNAM"), 0x6ED7_E048_u32.to_le_bytes())
            .zsub(T(b"ANAM"), "Sound\\Music\\PaletteLeaf.xwm"),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let subject = game.player();

    let selected = music::select(&mut game, id(0x800), subject, 0.5).unwrap();

    assert_eq!(selected.track, id(0x802));
    assert_eq!(
        selected.path.as_deref(),
        Some("sound/music/paletteleaf.xwm")
    );
    assert_eq!(selected.fade_in, 1.5);
    assert_eq!(selected.fade_out, 3.25);
}

fn installed_music_pair(data: &std::path::Path) -> (GlobalFormId, GlobalFormId, f32, f32) {
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path = data.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut game = Game::new(order);
    let player = game.player();
    let ids: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag::new(b"MUSC"))
        .map(|(id, _)| id)
        .collect();
    let mut eligible = Vec::new();
    for id in ids {
        if let Ok(track) = music::select(&mut game, id, player, 0.5) {
            if !track.flags.abrupt_transition && track.path.is_some() && track.fade_in > 0.2 {
                eligible.push(id);
            }
        }
    }
    for first in &eligible {
        let Ok(first_track) = music::select(&mut game, *first, player, 0.5) else {
            continue;
        };
        for second in &eligible {
            if first == second {
                continue;
            }
            let Ok(second_track) = music::select(&mut game, *second, player, 0.5) else {
                continue;
            };
            if first_track.path != second_track.path {
                return (*first, *second, first_track.fade_out, second_track.fade_in);
            }
        }
    }
    panic!("installed masters contain no pair of distinct eligible fading music types");
}

#[test]
fn aud_05_record_transition_fades_out_and_rejects_invalid_selection_without_stale_music() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed music transition check");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("SOE_GODOT_BIN not set, skipping installed music transition check");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let project = workspace.join("godot");
    let extension = project.join("soe.gdextension");
    let (first, second, first_fade_out, second_fade_in) = installed_music_pair(&data);
    let fixture = tempfile::tempdir().unwrap();
    let script = fixture.path().join("aud_05_record_transition.gd");
    std::fs::write(
        &script,
        r#"extends SceneTree
var failures := 0
func check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)
func wait_for_count(bank: Node, expected: int) -> bool:
	for _frame in range(600):
		if int(bank.call("music_start_count")) >= expected:
			return true
		await process_frame
	return false
func _initialize() -> void:
	call_deferred("run")
func run() -> void:
	if not ClassDB.class_exists("PlayerNode"):
		check(GDExtensionManager.load_extension(OS.get_environment("AUD_05_EXTENSION")) == GDExtensionManager.LOAD_STATUS_OK, "extension loads")
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	check(streamer.call("load_data", OS.get_environment("SKYRIM_DATA")), "installed data opens")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	check(player.call("use_streamer_data", streamer), "player shares installed data")
	check(player.call("enter_cell", "WhiterunBanneredMare", -125.0, -800.0, 120.0) > 0, "player enters test cell")
	check(player.call("start_gameplay"), "gameplay starts")
	var bank := player.get_node("SoundBank")
	var first_player := bank.get_child(16) as AudioStreamPlayer
	var second_player := bank.get_child(17) as AudioStreamPlayer
	var first_id_file := int(OS.get_environment("AUD_05_FIRST_FILE"))
	var first_id_object := int(OS.get_environment("AUD_05_FIRST_OBJECT"))
	var second_id_file := int(OS.get_environment("AUD_05_SECOND_FILE"))
	var second_id_object := int(OS.get_environment("AUD_05_SECOND_OBJECT"))
	check(player.call("select_music_type", first_id_file, first_id_object, 0.5), "first eligible record selects")
	check(await wait_for_count(bank, 1), "first record starts playback")
	check(player.call("select_music_type", second_id_file, second_id_object, 0.5), "second eligible record selects")
	check(await wait_for_count(bank, 2), "second record starts playback")
	var outgoing := first_player
	var incoming := second_player
	await process_frame
	var expected_outgoing_fade := float(OS.get_environment("AUD_05_FIRST_FADE_OUT"))
	if expected_outgoing_fade > 0.0:
		check(outgoing.playing and incoming.playing, "configured outgoing fade overlaps the incoming track")
		check(outgoing.volume_db < -0.01, "outgoing record fade begins below full volume")
	else:
		check(outgoing.volume_db <= -80.0, "zero outgoing fade mutes the old track")
		check(incoming.playing, "incoming record plays when outgoing fade is zero")
	check(not player.call("select_music_type", 65535, 4294967294, 0.5), "missing MUSC is rejected")
	check(not player.call("select_music_type", first_id_file, first_id_object, 2.0), "out-of-range draw is rejected")
	check(int(bank.call("music_start_count")) == 2, "invalid selections do not start a third track")
	await create_timer(0.2).timeout
	if expected_outgoing_fade > 0.0:
		check(outgoing.volume_db < -0.01, "outgoing track remains in its configured fade")
	await create_timer(maxf(float(OS.get_environment("AUD_05_FIRST_FADE_OUT")), float(OS.get_environment("AUD_05_SECOND_FADE_IN"))) + 0.2).timeout
	check(int(bank.call("music_playing_count")) == 1, "completed record transition leaves one music player active")
	check(incoming.playing, "latest selected record remains active")
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " AUD-05 record transition")
	quit(0 if failures == 0 else 1)
"#,
    )
    .unwrap();
    let output = Command::new(godot)
        .args(["--path"])
        .arg(project)
        .args(["--resolution", "640x360", "--script"])
        .arg(&script)
        .env("SKYRIM_DATA", &data)
        .env("AUD_05_EXTENSION", extension)
        .env("AUD_05_FIRST_FILE", first.file.to_string())
        .env("AUD_05_FIRST_OBJECT", first.object.to_string())
        .env("AUD_05_SECOND_FILE", second.file.to_string())
        .env("AUD_05_SECOND_OBJECT", second.object.to_string())
        .env("AUD_05_FIRST_FADE_OUT", first_fade_out.to_string())
        .env("AUD_05_SECOND_FADE_IN", second_fade_in.to_string())
        .output()
        .expect("Godot process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS"),
        "record-backed music transition failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
