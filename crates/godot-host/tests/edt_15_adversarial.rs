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

fn transform(z: f32) -> Transform {
    Transform {
        position: [12.0, -8.0, z],
        rotation: [0.25, -0.5, 1.75],
    }
}

fn fixture(data_fields: Vec<Vec<u8>>) -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let active_path = directory.path().join("Active.esp");
    fs::write(&base_path, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    let mut active = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    let mut reference = Record::new(Tag(*b"REFR"), 0x0100_0800)
        .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes().to_vec());
    for data in data_fields {
        reference.subrecords.push(soe_esm::plugin::Subrecord {
            tag: Tag(*b"DATA"),
            data,
        });
    }
    active.push_top(reference);
    fs::write(&active_path, active.to_bytes()).unwrap();
    let paths: [PathBuf; 2] = [base_path, active_path];
    let document = EditorDocument::open(&paths, "Active.esp").unwrap();
    (document, directory)
}

fn current(document: &EditorDocument) -> Option<Transform> {
    document
        .load_order()
        .winner(FORM)
        .and_then(|record| Refr::from_record(record).ok())
        .and_then(|reference| reference.transform)
}

#[test]
fn edt_15_negative_surface_height_preserves_horizontal_position_and_radians() {
    let original = transform(240.0);
    let (mut document, _directory) = fixture(vec![original.to_bytes().to_vec()]);

    document
        .set_reference_transform(
            FORM,
            original.drop_to_ground(-4096.25).unwrap().placed(),
        )
        .unwrap();

    assert_eq!(current(&document), Some(transform(-4096.25)));
    assert!(document.undo().unwrap());
    assert_eq!(current(&document), Some(original));
}

#[test]
fn edt_15_malformed_or_repeated_transform_data_is_rejected_without_undo() {
    for fields in [
        vec![vec![0; 23]],
        vec![transform(10.0).to_bytes().to_vec(), transform(20.0).to_bytes().to_vec()],
    ] {
        let (mut document, _directory) = fixture(fields);
        let before = document.active_plugin().clone();

        assert!(document
            .set_reference_transform(FORM, transform(-5.0))
            .is_err());
        assert_eq!(document.active_plugin(), &before);
        assert!(!document.undo().unwrap());
    }
}

#[test]
fn edt_15_missing_transform_data_cannot_create_an_undoable_placement() {
    let (mut document, _directory) = fixture(Vec::new());
    let before = document.active_plugin().clone();

    assert!(document
        .set_reference_transform(FORM, transform(0.0))
        .is_err());
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
}
