// SPDX-License-Identifier: GPL-3.0-or-later

//! Camera-to-Control compass motion without installed world assets.

#[test]
fn ui_29_windowed_camera_degree_steps_move_the_rendered_strip(
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let directory = tempfile::tempdir_in("/tmp")?;
    let plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    std::fs::write(directory.path().join("Ui05Config.esm"), plugin.to_bytes())?;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = std::process::Command::new(godot)
        .args([
            "--path",
            "godot",
            "--resolution",
            "640x360",
            "--script",
            "res://ui_29_compass_test.gd",
        ])
        .current_dir(&root)
        .env("SOE_UI_29_FIXTURE_DIR", directory.path())
        .output()?;
    let artifacts = root.join("target/ui-29");
    std::fs::create_dir_all(&artifacts)?;
    std::fs::write(artifacts.join("stdout.txt"), &output.stdout)?;
    std::fs::write(artifacts.join("stderr.txt"), &output.stderr)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stdout.contains("PASS ui_29 compass 361 samples"),
        "{stdout}\n{stderr}"
    );
    std::fs::copy(
        directory.path().join("compass.png"),
        artifacts.join("compass.png"),
    )?;
    Ok(())
}
