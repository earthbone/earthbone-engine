// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed-data console input regression runs the real walkthrough headless on Xvfb.

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[path = "common/isolated_project.rs"]
mod isolated_project;

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn console_overlay_walkthrough_opens_submits_history_and_closes() {
    let (Some(data), Some(godot)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
    ) else {
        return;
    };
    let extension = std::env::var_os("SOE_CONSOLE_EXTENSION").map(std::path::PathBuf::from);
    assert!(
        extension.as_ref().is_some_and(|path| path.is_file()),
        "build release extension through the run cargo wrapper and set SOE_CONSOLE_EXTENSION"
    );
    let project = isolated_project::project(
        &[
            "console_overlay_test.gd",
            "walkthrough.gd",
            "render_warmup.gd",
            "system_overlay.gd",
            "load_diagnostics.gd",
            "record_sky.gd",
            "record_sky.gdshader",
            "record_fog.gdshader",
        ],
        extension.as_deref(),
    );
    let result = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "console_overlay_test.gd"])
        .env("SKYRIM_DATA", data)
        .env("DISPLAY", ":99")
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .output();
    assert!(result.is_ok(), "could not start Godot: {result:?}");
    let Ok(result) = result else {
        return;
    };
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains("CONSOLE_OVERLAY_RESULT PASS"),
        "headless walkthrough {}\n{stdout}\n{stderr}",
        result.status
    );
}

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
fn r4_scene(script: &str, marker: &str, windowed: bool) {
    let (Some(data), Some(godot), Some(extension)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_GODOT_BIN"),
        std::env::var_os("SOE_CONSOLE_EXTENSION"),
    ) else {
        return;
    };
    let extension = std::path::Path::new(&extension);
    assert!(extension.is_file(), "release extension must exist");
    let project = isolated_project::project(
        &[
            script,
            "walkthrough.gd",
            "render_warmup.gd",
            "system_overlay.gd",
            "load_diagnostics.gd",
            "record_sky.gd",
            "record_sky.gdshader",
            "record_fog.gdshader",
        ],
        Some(extension),
    );
    let mut command = std::process::Command::new(godot);
    if windowed {
        command.args(["--rendering-method", "gl_compatibility"]);
    } else {
        command.arg("--headless");
    }
    let result = command
        .arg("--path")
        .arg(project.path())
        .args(["--script", script])
        .env("SKYRIM_DATA", data)
        .env("DISPLAY", ":99")
        .env("LIBGL_ALWAYS_SOFTWARE", "1")
        .env_remove("WAYLAND_DISPLAY")
        .env("XDG_DATA_HOME", project.path())
        .env("XDG_CACHE_HOME", project.path())
        .output();
    assert!(result.is_ok(), "could not start Godot: {result:?}");
    let Ok(result) = result else { return };
    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        result.status.success() && stdout.contains(marker),
        "{script} {}\n{stdout}\n{stderr}",
        result.status
    );
}

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn r4_console_text_uses_clipboards_mouse_selection_and_paging() {
    r4_scene("r4_console_text_test.gd", "R4_TEXT_RESULT PASS", true);
}

#[cfg(all(feature = "godot", target_os = "linux", target_arch = "x86_64"))]
#[test]
fn r4_coc_installs_queued_cell_while_console_pauses_gameplay() {
    r4_scene("r4_coc_paused_test.gd", "R4_PAUSED_RESULT PASS", false);
}
