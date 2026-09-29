// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::{EditorDocument, EditorError};
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const FIELD: Tag = Tag(*b"EDID");

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-01-adversarial-{}-{label}",
        std::process::id()
    ));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn fixture(label: &str, record: Record) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(record);
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

fn static_record() -> Record {
    Record::new(Tag(*b"STAT"), 0x800)
        .zsub(FIELD, "StoneMarker")
        .zsub(Tag(*b"FULL"), "Old value")
}

fn remove(paths: &[&PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_01_adversarial_glob_override_undo_and_save_reload() {
    let (mut document, base_path, patch_path) = fixture(
        "glob",
        Record::new(Tag(*b"GLOB"), FORM.object).zsub(FIELD, "WorldTimeScale"),
    );

    document
        .set_subrecord(FORM, FIELD, b"EditedGlobal\0".to_vec())
        .unwrap();

    let override_record = document.load_order().winner(FORM).unwrap();
    assert_eq!(override_record.tag, Tag(*b"GLOB"));
    assert_eq!(override_record.find(FIELD), Some(&b"EditedGlobal\0"[..]));

    let saved = document.save(&patch_path).unwrap();
    let reloaded = EditorDocument::open(
        &[base_path.clone(), patch_path.clone()],
        patch_path.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(saved.to_bytes(), reloaded.active_plugin().to_bytes());
    assert_eq!(
        reloaded.load_order().winner(FORM).unwrap().find(FIELD),
        Some(&b"EditedGlobal\0"[..])
    );

    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin().records.len(), 0);
    assert_eq!(
        document.load_order().winner(FORM).unwrap().find(FIELD),
        Some(&b"WorldTimeScale\0"[..])
    );
    remove(&[&base_path, &patch_path]);
}

#[test]
fn edt_01_adversarial_repeated_edits_update_one_override_and_undo_in_order() {
    let (mut document, base_path, patch_path) = fixture("repeated", static_record());

    document
        .set_subrecord(FORM, Tag(*b"FULL"), b"First\0".to_vec())
        .unwrap();
    document
        .set_subrecord(FORM, Tag(*b"FULL"), b"Second\0".to_vec())
        .unwrap();
    assert_eq!(document.active_plugin().records.len(), 1);
    assert!(document.undo().unwrap());
    assert_eq!(
        document
            .load_order()
            .winner(FORM)
            .unwrap()
            .find(Tag(*b"FULL")),
        Some(&b"First\0"[..])
    );
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin().records.len(), 0);
    assert_eq!(
        document
            .load_order()
            .winner(FORM)
            .unwrap()
            .find(Tag(*b"FULL")),
        Some(&b"Old value\0"[..])
    );
    remove(&[&base_path, &patch_path]);
}

#[test]
fn edt_01_adversarial_missing_form_does_not_change_document_or_undo_stack() {
    let (mut document, base_path, patch_path) = fixture("missing", static_record());
    let before = document.active_plugin().clone();
    let missing = GlobalFormId {
        file: 0,
        object: 0x999,
    };

    assert!(matches!(
        document.set_subrecord(missing, FIELD, b"Missing\0".to_vec()),
        Err(EditorError::MissingForm(_))
    ));
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    remove(&[&base_path, &patch_path]);
}

#[test]
fn edt_01_adversarial_duplicate_field_rejection_is_atomic() {
    let record = Record::new(Tag(*b"STAT"), FORM.object)
        .zsub(FIELD, "StoneMarker")
        .zsub(Tag(*b"FULL"), "First")
        .zsub(Tag(*b"FULL"), "Second");
    let (mut document, base_path, patch_path) = fixture("duplicate", record);
    let before = document.active_plugin().clone();

    assert!(matches!(
        document.set_subrecord(FORM, Tag(*b"FULL"), b"Edited\0".to_vec()),
        Err(EditorError::RepeatedSubrecord { .. })
    ));
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    remove(&[&base_path, &patch_path]);
}

#[test]
fn edt_01_adversarial_saved_override_reopens_with_same_resolved_winner() {
    let (mut document, base_path, patch_path) = fixture("reload", static_record());
    document
        .set_subrecord(FORM, Tag(*b"FULL"), b"Persisted\0".to_vec())
        .unwrap();
    let saved = document.save(&patch_path).unwrap();
    let reloaded = EditorDocument::open(
        &[base_path.clone(), patch_path.clone()],
        patch_path.file_name().unwrap().to_str().unwrap(),
    )
    .unwrap();

    assert_eq!(saved.to_bytes(), reloaded.active_plugin().to_bytes());
    assert_eq!(
        reloaded
            .load_order()
            .winner(FORM)
            .unwrap()
            .find(Tag(*b"FULL")),
        Some(&b"Persisted\0"[..])
    );
    remove(&[&base_path, &patch_path]);
}

#[test]
fn edt_01_adversarial_unlisted_owner_rejection_is_atomic() {
    let (base_path, patch_path) = paths("no_master");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(static_record());
    let patch = Plugin::new(FileHeader::default());
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let mut document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    let before = document.active_plugin().clone();

    assert!(matches!(
        document.set_subrecord(FORM, FIELD, b"Edited\0".to_vec()),
        Err(EditorError::UnsupportedEdit { .. })
    ));
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    remove(&[&base_path, &patch_path]);
}
