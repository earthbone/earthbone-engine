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
const VALUE: Tag = Tag(*b"FULL");

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-01-{}-{label}", std::process::id()));
    let base = root.with_extension("Base.esm");
    let patch = root.with_extension("Patch.esp");
    (base, patch)
}

fn fixture(label: &str) -> (EditorDocument, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(
        Record::new(Tag(*b"STAT"), 0x800)
            .zsub(Tag(*b"EDID"), "StoneMarker")
            .zsub(VALUE, "Old value"),
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "LocalMarker"));
    // The local record uses the patch's own master index and a distinct id.
    patch.records[0].form_id = 0x01000801;
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, patch_path)
}

#[test]
fn edt_01_load_order_opens_from_plugin_paths() {
    let (document, patch_path) = fixture("opens");
    assert!(document.active_name().ends_with("Patch.esp"));
    assert_eq!(document.load_order().files().len(), 2);
    let _ = fs::remove_file(paths("opens").0);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_01_override_is_created_for_winning_record() {
    let (mut document, patch_path) = fixture("override");
    document
        .set_subrecord(FORM, VALUE, b"New value\0".to_vec())
        .unwrap();
    assert_eq!(document.active_plugin().records.len(), 2);
    assert_eq!(document.active_plugin().records[1].form_id, 0x0000_0800);
    assert_eq!(
        document.active_plugin().records[1].find(VALUE),
        Some(&b"New value\0"[..])
    );
    assert_eq!(
        document.load_order().winner(FORM).unwrap().find(VALUE),
        Some(&b"New value\0"[..])
    );
    let _ = fs::remove_file(paths("override").0);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_01_override_edit_can_be_undone() {
    let (mut document, patch_path) = fixture("undo");
    let original = document.active_plugin().clone();
    document
        .set_subrecord(FORM, VALUE, b"New value\0".to_vec())
        .unwrap();
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    assert_eq!(
        document.load_order().winner(FORM).unwrap().find(VALUE),
        Some(&b"Old value\0"[..])
    );
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(paths("undo").0);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_01_saved_plugin_reloads_equal() {
    let (mut document, patch_path) = fixture("save");
    document
        .set_subrecord(FORM, VALUE, b"New value\0".to_vec())
        .unwrap();
    let saved = document.save(&patch_path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&patch_path).unwrap()).unwrap();
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    assert_eq!(reloaded.records.len(), 2);
    assert_eq!(reloaded.header.record_count, 2);
    assert_eq!(reloaded.records[1].find(VALUE), Some(&b"New value\0"[..]));
    let _ = fs::remove_file(paths("save").0);
    let _ = fs::remove_file(patch_path);
}
