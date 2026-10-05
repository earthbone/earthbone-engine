// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Exercises music and ambience decode latency, cache reuse and request replacement in Godot.

#[path = "common/aud_11.rs"]
mod aud_11;

use soe_bsa::{write_bsa, BsaFile, BsaVersion};
use soe_esm::{FileHeader, Plugin};
use std::path::{Path, PathBuf};
use std::process::Command;

fn write_wave(path: &Path, bytes: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let data_bytes = u32::try_from(bytes).unwrap();
    let mut file = std::fs::File::create(path).unwrap();
    use std::io::Write;
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())
        .unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16_u32.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&1_u16.to_le_bytes()).unwrap();
    file.write_all(&22_050_u32.to_le_bytes()).unwrap();
    file.write_all(&(22_050_u32 * 2).to_le_bytes()).unwrap();
    file.write_all(&2_u16.to_le_bytes()).unwrap();
    file.write_all(&16_u16.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_bytes.to_le_bytes()).unwrap();
    file.set_len(44 + bytes).unwrap();
}

fn write_fixture(data: &Path) {
    std::fs::create_dir_all(data).unwrap();
    std::fs::write(
        data.parent().unwrap().join("Skyrim_Default.ini"),
        "[General]\nsLanguage=ENGLISH\n[Archive]\nsResourceArchiveList=Skyrim - Voices_en0.bsa\n",
    )
    .unwrap();
    for master in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            data.join(master),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    let voices = write_bsa(
        &[BsaFile {
            path: "sound/voice/aud12/placeholder.wav",
            bytes: &[],
        }],
        BsaVersion::SkyrimSe,
    )
    .unwrap();
    std::fs::write(data.join("Skyrim - Voices_en0.bsa"), voices).unwrap();
    // Large sparse PCM files make a synchronous full-file decode visible without a codec fixture.
    write_wave(&data.join("Music/aud12_first.wav"), 2 * 1024 * 1024);
    write_wave(&data.join("Music/aud12_latest.wav"), 2 * 1024 * 1024);
    write_wave(&data.join("Music/aud12_ambience.wav"), 2 * 1024 * 1024);
    write_wave(&data.join("Music/aud12_short.wav"), 44_100);
    write_wave(&data.join("Music/aud12_cap_a.wav"), 34 * 1024 * 1024);
    write_wave(&data.join("Music/aud12_cap_b.wav"), 34 * 1024 * 1024);
}

#[test]
fn aud_12_music_and_ambience_requests_do_not_block_and_reuse_decodes() {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let project = aud_11::project(&[]);

    let fixture = tempfile::tempdir().unwrap();
    let data = fixture.path().join("Data");
    write_fixture(&data);
    let script = tempfile::Builder::new()
        .prefix("aud_12_adversarial_")
        .suffix(".gd")
        .tempfile_in(project.path())
        .unwrap();
    std::fs::write(
        script.path(),
        r#"extends SceneTree

var failures: Array[String] = []

func _initialize() -> void:
	call_deferred("run")

func elapsed_ms(callable: Callable) -> float:
	var start := Time.get_ticks_usec()
	callable.call()
	return float(Time.get_ticks_usec() - start) / 1000.0

func run() -> void:
	var bank = ClassDB.instantiate("SoundBank")
	get_root().add_child(bank)
	await process_frame
	if not bank.call("open_data", OS.get_environment("AUD_12_DATA")):
		printerr("fixture data failed: ", bank.call("last_error"))
		quit(1)
		return
	bank.call("play_music_track", "music/aud12_short.wav", 0.0, 0.0, true)
	for _frame in range(120):
		await process_frame
		if int(bank.call("music_start_count")) > 0:
			break
	var starts_before_requests := int(bank.call("music_start_count"))
	var first_ms := elapsed_ms(func(): bank.call("play_music_track", "music/aud12_first.wav", 0.0, 0.0, true))
	var latest_ms := elapsed_ms(func(): bank.call("play_music_track", "music/aud12_latest.wav", 0.0, 0.0, true))
	if int(bank.call("music_playing_count")) == 0:
		failures.append("current music stopped while its replacement decoded")
	var ambience_ms := elapsed_ms(func(): bank.call("play_ambience", "music/aud12_ambience.wav"))
	if first_ms >= 2.0:
		failures.append("music request blocked %.2f ms" % first_ms)
	if latest_ms >= 2.0:
		failures.append("replacement music request blocked %.2f ms" % latest_ms)
	if ambience_ms >= 2.0:
		failures.append("ambience request blocked %.2f ms" % ambience_ms)
	for _frame in range(120):
		await process_frame
		if int(bank.call("music_start_count")) > 0 and int(bank.call("ambience_start_count")) > 0:
			break
	if int(bank.call("music_start_count")) != starts_before_requests + 1:
		failures.append("superseded music request started %d tracks" % int(bank.call("music_start_count")))
	var decoded_before := int(bank.call("decoded_count"))
	bank.call("play_music_track", "music/aud12_latest.wav", 0.0, 0.0, true)
	await process_frame
	if int(bank.call("decoded_count")) != decoded_before:
		failures.append("cached music path decoded again")
	if int(bank.call("ambience_start_count")) != 1:
		failures.append("ambience did not start after decode")
	var effect_start := Time.get_ticks_usec()
	var short_effect: Variant = bank.call("play_effect", "music/aud12_short.wav", Vector3.ZERO)
	var effect_ms := float(Time.get_ticks_usec() - effect_start) / 1000.0
	var voice_start := Time.get_ticks_usec()
	var short_voice: Variant = bank.call("play_voice", "music/aud12_short.wav", Vector3.ZERO)
	var voice_ms := float(Time.get_ticks_usec() - voice_start) / 1000.0
	if int(short_effect) < 0 or int(short_voice) < 0:
		failures.append("small effect or voice decode stopped working")
	if effect_ms >= 2.0 or voice_ms >= 2.0:
		failures.append("cached short effect or voice request blocked %.2f/%.2f ms" % [effect_ms, voice_ms])
	if int(bank.call("effect_start_count")) != 1 or int(bank.call("voice_start_count")) != 1:
		failures.append("effect and voice start counters did not advance")
	for path in ["music/aud12_cap_a.wav", "music/aud12_cap_b.wav"]:
		var before := int(bank.call("decoded_count"))
		bank.call("play_music_track", path, 0.0, 0.0, true)
		for _frame in range(600):
			await process_frame
			if int(bank.call("decoded_count")) > before:
				break
	if int(bank.call("music_cache_bytes")) > 64 * 1024 * 1024:
		failures.append("decoded cache exceeded its 64 MiB limit")
	if failures.is_empty():
		print("AUD-12 PASS")
	else:
		for failure in failures:
			printerr("AUD-12 FAILURE: ", failure)
		quit(1 if not failures.is_empty() else 0)
		return
	quit(0)
"#,
    )
    .unwrap();

    let output = Command::new("timeout")
        .arg("45s")
        .arg(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--windowed", "--resolution", "640x360"])
        .arg("--script")
        .arg(script.path())
        .env("AUD_12_DATA", data)
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("AUD-12 PASS"),
        "AUD-12 Godot adversarial check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}

#[test]
fn aud_12_installed_prf12_walkthrough_keeps_audio_under_two_ms() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN").map(PathBuf::from) else {
        eprintln!("skipping: SOE_GODOT_BIN is not set");
        return;
    };
    let project = aud_11::project(&[
        "prf_12_walkthrough_test.gd",
        "walkthrough.gd",
        "render_warmup.gd",
        "system_overlay.gd",
    ]);
    let script = tempfile::Builder::new()
        .prefix("aud_12_prf12_")
        .suffix(".gd")
        .tempfile_in(project.path())
        .unwrap();
    std::fs::write(
        script.path(),
        r#"extends "res://prf_12_walkthrough_test.gd"

func _process(delta: float) -> bool:
	var result: bool = super._process(delta)
	if frames == 12:
		var report: Dictionary = player.call("system_frame_breakdown")
		var audio: Dictionary = report.get("audio", {})
		print("AUD-12 audio_ms=", audio.get("worst_ms", -1.0))
	return result
"#,
    )
    .unwrap();
    let output = Command::new("timeout")
        .arg("120s")
        .arg(godot)
        .args(["--path"])
        .arg(project.path())
        .args(["--resolution", "640x360", "--script"])
        .arg(script.path())
        .env("SKYRIM_DATA", data)
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let audio_ms = stdout
        .lines()
        .find_map(|line| {
            line.split_once("AUD-12 audio_ms=")
                .map(|(_, value)| value.trim())
        })
        .and_then(|value| value.parse::<f64>().ok());
    assert!(
        output.status.success() && audio_ms.is_some_and(|ms| ms <= 2.0),
        "installed PRF-12 audio budget failed, audio_ms={audio_ms:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
    println!(
        "AUD-12 installed PRF-12 worst audio frame: {:.3} ms",
        audio_ms.unwrap()
    );
}
