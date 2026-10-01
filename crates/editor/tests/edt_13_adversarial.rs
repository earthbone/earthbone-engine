// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::{EditorDocument, EditorError};
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const STAT_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const WEAP_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};
const MISSING_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x802,
};

fn fixture(label: &str, include_model: bool) -> (EditorDocument, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-13-adversarial-{}-{label}",
        std::process::id()
    ));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    let mut stat = Record::new(Tag(*b"STAT"), STAT_ID.object);
    if include_model {
        stat.subrecords.push(soe_esm::plugin::Subrecord::new(
            Tag(*b"MODL"),
            b"old.nif\0".to_vec(),
        ));
    }
    base.push_top(stat);
    base.push_top(Record::new(Tag(*b"WEAP"), WEAP_ID.object));
    let patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..FileHeader::default()
    });
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let doc = EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (doc, base_path, patch_path)
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_13_optional_model_field_can_be_added_and_undone() {
    let (mut doc, base, patch) = fixture("optional-field", false);
    assert_eq!(doc.stat_model_field(STAT_ID).unwrap().value, None);
    doc.set_stat_model_field(STAT_ID, "new.nif").unwrap();
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().value.as_deref(),
        Some("new.nif")
    );
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        patch.file_name().unwrap().to_str().unwrap()
    );
    assert!(doc.undo().unwrap());
    assert_eq!(doc.stat_model_field(STAT_ID).unwrap().value, None);
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_13_repeated_field_edits_undo_one_override_at_a_time() {
    let (mut doc, base, patch) = fixture("repeat-edit", true);
    doc.set_stat_model_field(STAT_ID, "first.nif").unwrap();
    doc.set_stat_model_field(STAT_ID, "second.nif").unwrap();
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().value.as_deref(),
        Some("second.nif")
    );
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        patch.file_name().unwrap().to_str().unwrap()
    );
    assert!(doc.undo().unwrap());
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().value.as_deref(),
        Some("first.nif")
    );
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        patch.file_name().unwrap().to_str().unwrap()
    );
    assert!(doc.undo().unwrap());
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().value.as_deref(),
        Some("old.nif")
    );
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_13_missing_and_wrong_type_forms_are_rejected_without_edits() {
    let (mut doc, base, patch) = fixture("invalid-target", true);
    let original = doc.active_plugin().clone();
    assert!(
        matches!(doc.stat_model_field(MISSING_ID), Err(EditorError::MissingForm(id)) if id == MISSING_ID)
    );
    assert!(
        matches!(doc.set_stat_model_field(MISSING_ID, "x.nif"), Err(EditorError::MissingForm(id)) if id == MISSING_ID)
    );
    assert!(matches!(
        doc.stat_model_field(WEAP_ID),
        Err(EditorError::UnsupportedEdit { .. })
    ));
    assert!(matches!(
        doc.set_stat_model_field(WEAP_ID, "x.nif"),
        Err(EditorError::UnsupportedEdit { .. })
    ));
    assert_eq!(doc.active_plugin(), &original);
    assert_eq!(
        doc.stat_model_field(STAT_ID).unwrap().winning_source,
        base.file_name().unwrap().to_str().unwrap()
    );
    cleanup(&[base, patch]);
}
