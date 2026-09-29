// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const OWNER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn document(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-edt-08-adversarial-{}-{label}",
        std::process::id()
    ));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active = patch_path.file_name().unwrap().to_string_lossy();
    let opened = EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active).unwrap();
    (opened, base_path, patch_path)
}

fn source() -> &'static str {
    "ScriptName EditorProbe\nInt Property Count Auto\nString Property Label Auto\nFunction Ping()\nReturn\nEndFunction"
}

#[test]
fn edt_08_recompiling_a_bound_script_preserves_existing_binding() {
    let (mut document, base_path, patch_path) = document("repeat-adversarial");
    document
        .compile_script_for_form(OWNER, "EditorProbe.psc", source())
        .unwrap();
    let before = document.active_plugin().records[0]
        .find(Tag(*b"VMAD"))
        .unwrap()
        .to_vec();
    let changed_source = "ScriptName EditorProbe\nInt Property Count Auto\nString Property Label Auto\nInt Property Extra Auto\nFunction Ping()\nReturn\nEndFunction";
    let second = document.compile_script_for_form(OWNER, "EditorProbe.psc", changed_source);

    assert!(second.is_err());
    assert_eq!(document.active_plugin().records.len(), 1);
    assert_eq!(
        document.active_plugin().records[0].find(Tag(*b"VMAD")),
        Some(before.as_slice())
    );
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}
