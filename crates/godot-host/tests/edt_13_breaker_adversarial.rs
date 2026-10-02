// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const STAT: Tag = Tag(*b"STAT");
const MODL: Tag = Tag(*b"MODL");

fn document() -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(
        Record::new(STAT, FORM.object)
            .zsub(Tag(*b"EDID"), "StoneMarker")
            .zsub(MODL, "Architecture\\Stone\\Old.nif"),
    );
    let patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_owned()],
        ..Default::default()
    });
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();
    (document, directory)
}

#[test]
fn edt_13_typed_model_edit_updates_winner_and_undo_restores_source() {
    let (mut document, _directory) = document();
    let original = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        original.value.as_deref(),
        Some("Architecture\\Stone\\Old.nif")
    );
    assert_eq!(original.winning_source, "Base.esm");

    document
        .set_stat_model_field(FORM, "Architecture\\Stone\\New.nif")
        .unwrap();
    let edited = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        edited.value.as_deref(),
        Some("Architecture\\Stone\\New.nif")
    );
    assert_eq!(edited.winning_source, "Patch.esp");
    assert_eq!(document.active_plugin().records.len(), 1);

    assert!(document.undo().unwrap());
    let undone = document.stat_model_field(FORM).unwrap();
    assert_eq!(
        undone.value.as_deref(),
        Some("Architecture\\Stone\\Old.nif")
    );
    assert_eq!(undone.winning_source, "Base.esm");
    assert!(document.active_plugin().records.is_empty());
}

#[test]
fn edt_13_invalid_typed_edits_leave_winner_and_undo_stack_unchanged() {
    let (mut document, _directory) = document();
    let initial_active = document.active_plugin().clone();
    let wrong_type = GlobalFormId {
        file: 0,
        object: 0x801,
    };

    assert!(document
        .set_stat_model_field(FORM, "bad\0path.nif")
        .is_err());
    assert!(document
        .set_stat_model_field(wrong_type, "mesh.nif")
        .is_err());
    assert_eq!(document.active_plugin(), &initial_active);
    assert_eq!(
        document.stat_model_field(FORM).unwrap().winning_source,
        "Base.esm"
    );
    assert!(!document.undo().unwrap());
}

#[test]
fn edt_13_repeating_same_edit_keeps_one_undo_step() {
    let (mut document, _directory) = document();
    let replacement = "Architecture\\Stone\\New.nif";

    document.set_stat_model_field(FORM, replacement).unwrap();
    document.set_stat_model_field(FORM, replacement).unwrap();
    assert!(document.undo().unwrap());
    assert_eq!(
        document.stat_model_field(FORM).unwrap().value.as_deref(),
        Some("Architecture\\Stone\\Old.nif")
    );
    assert!(!document.undo().unwrap());
}

#[test]
fn edt_13_inspector_displays_the_selected_forms_winning_plugin() {
    let inspector = include_str!("../../../godot/addons/soe_forms/form_inspector.gd");
    let plugin = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");

    assert!(inspector.contains("add_custom_control(winning_source_control(object))"));
    assert!(inspector.contains("source.text = object.get(\"winning_plugin\")"));
    assert!(plugin.contains("add_inspector_plugin(form_inspector)"));
    assert!(plugin.contains("remove_inspector_plugin(form_inspector)"));
}
