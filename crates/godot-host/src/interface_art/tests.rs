// SPDX-License-Identifier: GPL-3.0-or-later

//! UI-27 discovery fixtures avoid changing the process environment.

use super::*;

fn executable_file(directory: &Path, name: &str) -> PathBuf {
    let file = directory.join(name);
    std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    file
}

fn selected_path(selected: Option<Decompiler>) -> PathBuf {
    match selected.unwrap() {
        Decompiler::Jar(path) | Decompiler::Program(path) => path,
    }
}

#[test]
fn ui_27_jar_override_precedes_launcher_tools_and_path() {
    let temp = tempfile::tempdir().unwrap();
    executable_file(temp.path(), "java");
    let launcher = executable_file(temp.path(), "ffdec");
    let jar = temp.path().join("chosen.jar");
    std::fs::write(&jar, []).unwrap();
    let dirs = [temp.path().to_path_buf()];
    assert!(matches!(
        discover(Some(&jar), Some(&launcher), &dirs, &dirs),
        Some(Decompiler::Jar(path)) if path == jar
    ));
}

#[test]
fn ui_27_launcher_override_resolves_path_and_precedes_tools() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = executable_file(temp.path(), "chosen");
    executable_file(temp.path(), "ffdec");
    let dirs = [temp.path().to_path_buf()];
    assert_eq!(
        selected_path(discover(None, Some(Path::new("chosen")), &dirs, &dirs)),
        launcher
    );
}

#[test]
fn ui_27_tools_directory_finds_cli_jar_and_requires_installed_java() {
    let temp = tempfile::tempdir().unwrap();
    let ffdec = temp.path().join("FFDec");
    std::fs::create_dir(&ffdec).unwrap();
    let jar = ffdec.join("ffdec-cli.jar");
    std::fs::write(&jar, []).unwrap();
    let dirs = [temp.path().to_path_buf()];
    assert!(discover(None, None, &dirs, &dirs).is_none());
    executable_file(temp.path(), "java");
    assert_eq!(selected_path(discover(None, None, &dirs, &dirs)), jar);
}

#[test]
fn ui_27_path_finds_cli_launcher_without_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = executable_file(temp.path(), "ffdec-cli");
    assert_eq!(
        selected_path(discover(None, None, &[], &[temp.path().to_path_buf()])),
        launcher
    );
}

#[test]
fn ui_27_invalid_overrides_fall_through_to_valid_tools() {
    let temp = tempfile::tempdir().unwrap();
    let launcher = executable_file(temp.path(), "ffdec.sh");
    let missing = temp.path().join("missing.jar");
    assert_eq!(
        selected_path(discover(
            Some(&missing),
            Some(&missing),
            &[temp.path().to_path_buf()],
            &[]
        )),
        launcher
    );
}

#[test]
fn ui_27_absent_tools_and_nonexecutable_files_return_no_decompiler() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("ffdec"), "not executable").unwrap();
    assert!(discover(None, None, &[], &[temp.path().to_path_buf()]).is_none());
    assert!(discover(None, None, &[], &[]).is_none());
}

#[test]
fn ui_27_configured_tools_precedes_path_and_accepts_direct_jar() {
    let tools = tempfile::tempdir().unwrap();
    let path = tempfile::tempdir().unwrap();
    executable_file(path.path(), "java");
    executable_file(path.path(), "ffdec");
    let jar = tools.path().join("configured.jar");
    std::fs::write(&jar, []).unwrap();
    assert_eq!(
        selected_path(discover(
            None,
            None,
            std::slice::from_ref(&jar),
            &[path.path().to_path_buf()]
        )),
        jar
    );
}

#[test]
fn ui_30_missing_java_notice_uses_the_live_menu_fallback() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return;
    };
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let captures = workspace.join("target/ui-30");
    std::fs::create_dir_all(&captures).unwrap();
    let output = std::process::Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ui_30_missing_java_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("DISPLAY", ":99")
        .env_remove("WAYLAND_DISPLAY")
        .env(
            "SOE_UI_30_NOTICE_CAPTURE",
            captures.join("missing-java.png"),
        )
        .env("SOE_JAVA", workspace.join("target/absent-ui-30-java"))
        .env("SOE_THREADED_SIMULATION", "0")
        .env_remove("SOE_INTERFACE_CACHE")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        captures.join("missing-java.log"),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    assert!(
        output.status.success()
            && stdout.contains("UI-30 NOTICE PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{}\n{stdout}\n{stderr}",
        output.status
    );
}
