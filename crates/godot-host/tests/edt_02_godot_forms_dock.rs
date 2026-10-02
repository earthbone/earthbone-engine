// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{Plugin, Tag};
use std::path::PathBuf;
use std::process::Command;

fn godot_binary() -> Option<PathBuf> {
    std::env::var_os("SOE_GODOT_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            Command::new("godot")
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|_| PathBuf::from("godot"))
        })
}

#[test]
fn edt_02_godot_editor_filters_loaded_forms_without_eager_resources() {
    let Some(godot) = godot_binary() else {
        eprintln!("skipping: set SOE_GODOT_BIN to a Godot 4.4 or newer executable");
        return;
    };
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.join("../..");
    let project = workspace.join("godot");
    let extension = workspace.join("target/release/libsoe_godot_host.so");
    if !extension.exists() {
        eprintln!("skipping: build the release GDExtension before running this test");
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let base_path = fixture.path().join("Base.esm");
    let patch_path = fixture.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "OakTree"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x801).zsub(Tag(*b"EDID"), "OakDoor"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x802).zsub(Tag(*b"EDID"), "PineTree"));
    base.push_top(Record::new(Tag(*b"MISC"), 0x803).zsub(Tag(*b"EDID"), "OakMisc"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "OakTreeWinning"));
    std::fs::write(&base_path, base.to_bytes()).unwrap();
    std::fs::write(&patch_path, patch.to_bytes()).unwrap();

    let output = Command::new(godot)
        .args(["--editor", "--path"])
        .arg(&project)
        .args(["--resolution", "640x360", "--quit-after", "900"])
        .env("SOE_EDT_02_RUNTIME_TEST", "1")
        .env("SOE_EDT_02_BASE", &base_path)
        .env("SOE_EDT_02_PATCH", &patch_path)
        .env("SOE_EDT_02_ACTIVE", "Patch.esp")
        .output()
        .expect("Godot editor process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("EDT-02 PASS"),
        "Godot editor forms dock check failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
