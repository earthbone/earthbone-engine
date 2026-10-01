// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-13-{}-{label}", std::process::id()));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(
        Record::new(Tag(*b"STAT"), 0x800)
            .zsub(Tag(*b"EDID"), "StoneMarker")
            .zsub(Tag(*b"MODL"), "Architecture\\Stone\\Old.nif"),
    );
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
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_13_typed_field_edit_creates_undoable_override_and_reports_winner() {
    let (mut document, base, patch) = fixture("field-edit");

    let initial = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        initial.value.as_deref(),
        Some("Architecture\\Stone\\Old.nif")
    );
    assert_eq!(
        initial.winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );

    document
        .set_stat_model_field(FORM, "Architecture\\Stone\\New.nif")
        .unwrap();
    assert_eq!(document.active_plugin().records.len(), 1);
    let edited = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        edited.value.as_deref(),
        Some("Architecture\\Stone\\New.nif")
    );
    assert_eq!(
        edited.winning_source,
        patch.file_name().unwrap().to_str().unwrap()
    );
    assert_eq!(
        document
            .load_order()
            .winner(FORM)
            .unwrap()
            .find(Tag(*b"MODL")),
        Some(&b"Architecture\\Stone\\New.nif\0"[..])
    );

    assert!(document.undo().unwrap());
    let undone = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        undone.value.as_deref(),
        Some("Architecture\\Stone\\Old.nif")
    );
    assert_eq!(
        undone.winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );
    assert!(document.active_plugin().records.is_empty());
    assert!(!document.undo().unwrap());

    cleanup(&[base, patch]);
}

#[test]
fn edt_13_typed_model_field_rejects_nul_without_mutating_document() {
    let (mut document, base, patch) = fixture("nul");
    let original = document.active_plugin().clone();

    assert!(document
        .set_stat_model_field(FORM, "bad\0path.nif")
        .is_err());
    assert_eq!(document.active_plugin(), &original);
    assert_eq!(
        document.stat_model_field(FORM).unwrap().winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );

    cleanup(&[base, patch]);
}
