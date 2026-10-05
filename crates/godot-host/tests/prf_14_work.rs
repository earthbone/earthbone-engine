// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]

//! Installed host counter checks using Cargo's extension in an isolated project.

#[test]
fn prf_14_godot_counts_live_render_instances_and_resets_work_windows() {
    if std::env::var_os("SKYRIM_DATA").is_none() {
        return;
    }
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // CONTEXT: Cargo test does not refresh the release library Godot loads.
    let build = std::process::Command::new(env!("CARGO"))
        .current_dir(&workspace)
        .args(["build", "-p", "soe-godot-host", "--release", "--target-dir"])
        .arg(workspace.join("target"))
        .output()
        .expect("release extension build starts");
    assert!(
        build.status.success(),
        "release extension build failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    let release_project = workspace.join("godot");
    // CONTEXT: A fresh checkout needs an import to register its GDExtension.
    let import = std::process::Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(&release_project)
        .arg("--import")
        .output()
        .expect("Godot project import starts");
    assert!(
        import.status.success(),
        "Godot project import failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&import.stdout),
        String::from_utf8_lossy(&import.stderr)
    );
    let release_output = std::process::Command::new(&godot)
        .args(["--headless", "--path"])
        .arg(&release_project)
        .args(["--script", "prf_14_work_test.gd"])
        .output()
        .unwrap();
    let release_stdout = String::from_utf8_lossy(&release_output.stdout);
    let release_stderr = String::from_utf8_lossy(&release_output.stderr);
    assert!(
        release_output.status.success(),
        "{release_stdout}\n{release_stderr}"
    );
    assert!(
        release_stdout.contains("RESULT: PASS"),
        "{release_stdout}\n{release_stderr}"
    );
    assert!(release_stdout.contains("PRF_14_COUNTS"), "{release_stdout}");
    assert!(!release_stderr.contains("SCRIPT ERROR"), "{release_stderr}");

    let executable = std::env::current_exe().unwrap();
    let library = executable.parent().unwrap().join("libsoe_godot_host.so");
    assert!(library.is_file(), "missing extension {}", library.display());
    let project = tempfile::tempdir_in(&workspace).unwrap();
    std::fs::write(
        project.path().join("project.godot"),
        "config_version=5\n[physics]\n3d/physics_engine=\"Jolt Physics\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("soe.gdextension"),
        format!(
            "[configuration]\nentry_symbol=\"gdext_rust_init\"\ncompatibility_minimum=\"4.4\"\n[libraries]\nlinux.debug.x86_64=\"{}\"\nlinux.release.x86_64=\"{}\"\n",
            library.display(),
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
    std::fs::copy(
        workspace.join("godot/prf_14_work_test.gd"),
        project.path().join("prf_14_work_test.gd"),
    )
    .unwrap();
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "prf_14_work_test.gd"])
        .env("XDG_DATA_HOME", project.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(stdout.contains("RESULT: PASS"), "{stdout}\n{stderr}");
    assert!(stdout.contains("PRF_14_COUNTS"), "{stdout}");
    assert!(!stderr.contains("SCRIPT ERROR"), "{stderr}");
}
