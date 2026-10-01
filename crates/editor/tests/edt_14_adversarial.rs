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

fn transform(x: f32) -> Transform {
    Transform {
        position: [x, x + 1.0, x + 2.0],
        rotation: [x / 10.0, x / 5.0, x / 2.0],
    }
}

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-14-adversarial-{}-{label}",
        std::process::id()
    ));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");
    let mut patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(Tag(*b"REFR"), 0x0100_0800)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes())
            .sub(Tag(*b"DATA"), transform(10.0).to_bytes()),
    );
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, Plugin::new(FileHeader::default()).to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn current(document: &EditorDocument) -> Transform {
    Refr::from_record(document.load_order().winner(FORM).unwrap())
        .unwrap()
        .transform
        .unwrap()
}

#[test]
fn edt_14_repeated_gizmo_commits_undo_in_reverse_order() {
    let (mut document, base, patch) = fixture("undo-sequence");
    let original = transform(10.0);
    let first = transform(20.0);
    let second = transform(30.0);
    document.set_reference_transform(FORM, first).unwrap();
    document.set_reference_transform(FORM, second).unwrap();

    assert_eq!(current(&document), second);
    assert!(document.undo().unwrap());
    assert_eq!(current(&document), first);
    assert!(document.undo().unwrap());
    assert_eq!(current(&document), original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}

#[test]
fn edt_14_missing_form_does_not_change_placement_or_create_undo() {
    let (mut document, base, patch) = fixture("missing");
    let before = document.active_plugin().clone();
    let absent = GlobalFormId {
        file: 1,
        object: 0x801,
    };

    assert!(document
        .set_reference_transform(absent, transform(50.0))
        .is_err());
    assert_eq!(document.active_plugin(), &before);
    assert!(!document.undo().unwrap());
    assert_eq!(current(&document), transform(10.0));
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}
