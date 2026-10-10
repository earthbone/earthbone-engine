// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-27 runs the installed Java/FFDec and the live container menu in place.

use std::path::{Path, PathBuf};
use std::process::Command;

const FFDEC: &str = "/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/SkyUI-Community/tools/FFDec";

fn capture_root() -> PathBuf {
    let root = std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"),
        PathBuf::from,
    );
    root.join("ui-27")
}

#[test]
fn ui_27_vanilla_container_actions_export_with_installed_java() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipped");
        return;
    };
    let archive =
        soe_bsa::Archive::open(&PathBuf::from(data).join("Skyrim - Interface.bsa")).unwrap();
    let bytes = archive.read("interface/containermenu.swf").unwrap();
    assert!(soe_swf_convert::convert(&bytes).unwrap().action_blocks > 0);
    let source = tempfile::tempdir_in("/tmp").unwrap();
    std::fs::create_dir(source.path().join("interface")).unwrap();
    std::fs::write(source.path().join("interface/containermenu.swf"), bytes).unwrap();
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(source.path()).unwrap();
    let output = tempfile::tempdir_in("/tmp").unwrap();
    let cache = soe_swf_convert::build_cache(
        &vfs,
        &["Skyrim.esm".into()],
        output.path(),
        &soe_swf_convert::Decompiler::Jar(Path::new(FFDEC).join("ffdec-cli.jar")),
    )
    .unwrap();
    let movie = cache.movies.get("interface/containermenu.swf").unwrap();
    let scripts = movie.parent().unwrap().join("as2-reference");
    let mut pending = vec![scripts];
    let mut exported = 0;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "as") {
                exported += 1;
            }
        }
    }
    assert!(
        exported > 0,
        "FFDec did not export vanilla container scripts"
    );
}

fn scene(data: &Path, tools: &Path, jar: Option<&Path>, bundled: bool, label: &str) -> String {
    let godot = std::env::var_os("SOE_GODOT_BIN").expect("SOE_GODOT_BIN is required");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let captures = capture_root();
    std::fs::create_dir_all(&captures).unwrap();
    let capture = captures.join(format!("{label}.png"));
    let cache = tempfile::tempdir_in("/tmp").unwrap();
    let mut command = Command::new(godot);
    command
        .arg("--path")
        .arg(workspace.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "ui_27_container_panel_test.gd",
        ])
        .env("SKYRIM_DATA", data)
        .env("SOE_LOG", "info")
        .env("HOME", tools)
        .env("SOE_UI_27_TOOLS", tools)
        .env("SOE_UI_27_CAPTURE", capture.as_os_str())
        .env("SOE_UI_27_BUNDLED", if bundled { "1" } else { "0" })
        .env("SOE_INTERFACE_CACHE_ROOT", cache.path())
        .env_remove("SOE_FFDEC_JAR")
        .env_remove("SOE_FFDEC")
        .env_remove("SOE_TOOLS_PATH")
        .env_remove("SOE_INTERFACE_CACHE");
    if let Some(jar) = jar {
        command.env("SOE_FFDEC_JAR", jar);
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    std::fs::write(
        captures.join(format!("{label}.log")),
        format!("{stdout}\n{stderr}"),
    )
    .unwrap();
    assert!(
        output.status.success()
            && stdout.contains("UI-27 PASS")
            && !stderr.contains("SCRIPT ERROR"),
        "{label}: status={}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(capture.is_file(), "missing {label} capture");
    format!("{stdout}\n{stderr}")
}

#[test]
fn ui_27_container_scene_preserves_panel_without_ffdec_and_after_conversion_failure() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipped");
        return;
    };
    if std::env::var_os("SOE_GODOT_BIN").is_none() {
        eprintln!("SOE_GODOT_BIN unset, skipped");
        return;
    }
    let data = PathBuf::from(data);
    let empty = tempfile::tempdir_in("/tmp").unwrap();
    let absent = scene(&data, empty.path(), None, true, "without-ffdec");
    assert_eq!(
        absent
            .matches("FFDec unavailable, using bundled menu theme")
            .count(),
        1
    );
    let broken = empty.path().join("broken.jar");
    std::fs::write(&broken, "invalid jar").unwrap();
    let failure = scene(&data, empty.path(), Some(&broken), true, "failed-ffdec");
    assert_eq!(failure.matches("with installed Java").count(), 1);
    assert_eq!(failure.matches("interface art cache failed").count(), 1);
    let found = scene(&data, Path::new(FFDEC), None, false, "with-ffdec");
    assert_eq!(found.matches("with installed Java").count(), 1);
    assert!(found.contains("ffdec-cli.jar"));
    let bounds = |log: &str| {
        log.lines()
            .find(|line| line.starts_with("UI-27 PANEL "))
            .unwrap()
            .to_owned()
    };
    assert_eq!(bounds(&absent), bounds(&found));
    assert_eq!(bounds(&absent), bounds(&failure));
}
