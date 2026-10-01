// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::refr::Refr;
use soe_esm::records::Transform;
use soe_esm::{GlobalFormId, Plugin, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 1,
    object: 0x800,
};

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-14-{}-{label}", std::process::id()));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn transform(position: [f32; 3], rotation: [f32; 3]) -> Transform {
    Transform { position, rotation }
}

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let base = Plugin::new(FileHeader::default());
    let mut patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    let initial = transform([10.0, 20.0, 30.0], [0.1, 0.2, 0.3]);
    let mut reference = Record::new(Tag(*b"REFR"), 0x0100_0800);
    reference.subrecords.push(soe_esm::Subrecord::new(
        Tag(*b"NAME"),
        0x800u32.to_le_bytes(),
    ));
    reference
        .subrecords
        .push(soe_esm::Subrecord::new(Tag(*b"DATA"), initial.to_bytes()));
    patch.push_top(reference);
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn winning_transform(document: &EditorDocument) -> Transform {
    let record = document.load_order().winner(FORM).unwrap();
    Refr::from_record(record).unwrap().transform.unwrap()
}

#[test]
fn edt_14_gizmo_operation_updates_winning_reference_transform() {
    let (mut document, base_path, patch_path) = fixture("edit");
    let moved = transform([-4.0, 50.0, 80.0], [0.4, 0.5, 0.6]);

    document.set_reference_transform(FORM, moved).unwrap();

    assert_eq!(winning_transform(&document), moved);
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_14_undo_restores_record_transform_used_for_rendering() {
    let (mut document, base_path, patch_path) = fixture("undo");
    let original = transform([10.0, 20.0, 30.0], [0.1, 0.2, 0.3]);
    document
        .set_reference_transform(FORM, transform([99.0, 98.0, 97.0], [1.0, 2.0, 3.0]))
        .unwrap();

    assert!(document.undo().unwrap());

    assert_eq!(winning_transform(&document), original);
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}

#[test]
fn edt_14_saved_reference_placement_round_trips_and_undo_saves_original() {
    let (mut document, base_path, patch_path) = fixture("save");
    let moved = transform([111.0, -222.0, 333.0], [0.0, 1.25, 2.5]);
    document.set_reference_transform(FORM, moved).unwrap();
    let saved = document.save(&patch_path).unwrap();
    let saved_record = saved
        .records_of(Tag(*b"REFR"))
        .next()
        .expect("saved reference is present");
    assert_eq!(
        Refr::from_record(saved_record).unwrap().transform,
        Some(moved)
    );

    assert!(document.undo().unwrap());
    let undone = document.save(&patch_path).unwrap();
    let undone_record = undone
        .records_of(Tag(*b"REFR"))
        .next()
        .expect("undone reference is present");
    assert_eq!(
        Refr::from_record(undone_record).unwrap().transform,
        Some(transform([10.0, 20.0, 30.0], [0.1, 0.2, 0.3]))
    );
    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}
