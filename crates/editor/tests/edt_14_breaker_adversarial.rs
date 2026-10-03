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

fn pose(seed: f32) -> Transform {
    Transform {
        position: [seed, seed + 11.0, seed - 23.0],
        rotation: [seed / 10.0, seed / 20.0, seed / 30.0],
    }
}

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-14-breaker-{}-{label}",
        std::process::id()
    ));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn fixture(label: &str, data_fields: Vec<Vec<u8>>) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let mut reference =
        Record::new(Tag(*b"REFR"), 0x0100_0800).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes());
    for data in data_fields {
        reference
            .subrecords
            .push(soe_esm::Subrecord::new(Tag(*b"DATA"), data));
    }
    let mut patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    patch.push_top(reference);
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn valid_fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    fixture(label, vec![pose(10.0).to_bytes().to_vec()])
}

fn cleanup(base: PathBuf, patch: PathBuf) {
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}

fn winning_transform(document: &EditorDocument) -> Transform {
    Refr::from_record(document.load_order().winner(FORM).unwrap())
        .unwrap()
        .transform
        .unwrap()
}

#[test]
fn edt_14_undo_after_repeated_edits_saves_and_reopens_the_prior_placement() {
    let (mut document, base, patch) = valid_fixture("undo-reopen");
    let first = pose(-40.25);
    let second = pose(87.5);
    document.set_reference_transform(FORM, first).unwrap();
    document.set_reference_transform(FORM, second).unwrap();

    assert!(document.undo().unwrap());
    assert_eq!(winning_transform(&document), first);
    document.save(&patch).unwrap();
    let active_name = patch.file_name().unwrap().to_string_lossy();
    let reopened = EditorDocument::open(&[base.clone(), patch.clone()], &active_name).unwrap();
    assert_eq!(winning_transform(&reopened), first);

    cleanup(base, patch);
}

#[test]
fn edt_14_missing_transform_data_is_rejected_without_edit_or_undo() {
    let (mut document, base, patch) = fixture("missing-data", vec![]);
    let before = document.active_plugin().clone();

    assert!(document.set_reference_transform(FORM, pose(30.0)).is_err());
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    assert!(
        Refr::from_record(document.load_order().winner(FORM).unwrap())
            .unwrap()
            .transform
            .is_none()
    );

    cleanup(base, patch);
}

#[test]
fn edt_14_short_transform_data_is_rejected_without_edit_or_undo() {
    let (mut document, base, patch) = fixture("short-data", vec![vec![0; 23]]);
    let before = document.active_plugin().clone();

    assert!(document.set_reference_transform(FORM, pose(30.0)).is_err());
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    assert_eq!(
        document
            .load_order()
            .winner(FORM)
            .unwrap()
            .find(Tag(*b"DATA")),
        Some(&[0; 23][..])
    );

    cleanup(base, patch);
}

#[test]
fn edt_14_repeated_transform_data_is_rejected_without_edit_or_undo() {
    let (mut document, base, patch) = fixture(
        "repeated-data",
        vec![
            pose(10.0).to_bytes().to_vec(),
            pose(20.0).to_bytes().to_vec(),
        ],
    );
    let before = document.active_plugin().clone();

    assert!(document.set_reference_transform(FORM, pose(30.0)).is_err());
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    let data: Vec<_> = document
        .load_order()
        .winner(FORM)
        .unwrap()
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.tag == Tag(*b"DATA"))
        .map(|subrecord| subrecord.data.clone())
        .collect();
    assert_eq!(
        data,
        vec![
            pose(10.0).to_bytes().to_vec(),
            pose(20.0).to_bytes().to_vec()
        ]
    );

    cleanup(base, patch);
}
