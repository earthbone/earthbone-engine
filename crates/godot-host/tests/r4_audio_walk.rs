// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Round 4 confirmations use the free walk scene and the installed load order.

#[path = "common/aud_11.rs"]
mod aud_11;

#[test]
fn r4_installed_free_walk_records_animation_footstep_descriptors(
) -> Result<(), Box<dyn std::error::Error>> {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_GODOT_BIN unset, installed scene skipped");
        return Ok(());
    };
    let project = aud_11::project(&["r4_audio_walk.gd"]);
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let library = aud_11::prepared_extension::require(&workspace);
    let snapshot = project.path().join("tested_host.so");
    assert_eq!(
        std::fs::copy(&library, &snapshot)?,
        std::fs::metadata(&library)?.len()
    );
    let descriptor = project.path().join("soe.gdextension");
    let configuration = std::fs::read_to_string(&descriptor)?;
    std::fs::write(
        &descriptor,
        configuration.replace(
            library
                .to_str()
                .ok_or("extension source path is not UTF-8")?,
            snapshot
                .to_str()
                .ok_or("extension snapshot path is not UTF-8")?,
        ),
    )?;
    eprintln!("R4 scene uses frozen extension {}", snapshot.display());
    let cell = std::env::var("R4_AUDIO_CELL").unwrap_or_else(|_| "WhiterunBanneredMare".into());
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "res://r4_audio_walk.gd"])
        .env("SKYRIM_DATA", data)
        .env("R4_AUDIO_CELL", &cell)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stdout.lines().filter(|line| line.starts_with("R4 ")) {
        eprintln!("{line}");
    }
    assert!(
        output.status.success()
            && stdout.contains(&format!("R4 FOOTSTEPS cell={cell}"))
            && stdout.contains("WALKTHROUGH stopped"),
        "free walk failed: {}\n{stdout}\n{stderr}",
        output.status
    );
    Ok(())
}
