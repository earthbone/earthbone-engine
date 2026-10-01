// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::{Heights, Land, VERTS, VERTS_PER_SIDE};
use soe_esm::{GlobalFormId, Plugin, Tag};

const LAND_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn fixture(label: &str) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-editor-edt-05-adversarial-{}-{label}.esp",
        std::process::id()
    ));
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"LAND"), LAND_FORM.object)
            .sub(Tag(*b"DATA"), 1u32.to_le_bytes())
            .sub(Tag(*b"VHGT"), Heights::encode(&vec![0.0; VERTS]).to_bytes()),
    );
    let _ = fs::remove_file(&path);
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active_name = path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(std::slice::from_ref(&path), &active_name).unwrap();
    (document, path)
}

#[test]
fn edt_05_repeated_dabs_accumulate_and_each_undo_restores_one_stroke() {
    let (mut document, path) = fixture("repeated");
    let center = [2048.0, 2048.0];
    document
        .brush_land_heights(LAND_FORM, center, 256.0, 80.0)
        .unwrap();
    document
        .brush_land_heights(LAND_FORM, center, 256.0, 80.0)
        .unwrap();

    let get_center = |doc: &EditorDocument| {
        Land::from_record(doc.load_order().winner(LAND_FORM).unwrap())
            .unwrap()
            .heights
            .unwrap()
            .decode()[16 * VERTS_PER_SIDE + 16]
    };
    assert_eq!(get_center(&document), 160.0);
    assert!(document.undo().unwrap());
    assert_eq!(get_center(&document), 80.0);
    assert!(document.undo().unwrap());
    assert_eq!(get_center(&document), 0.0);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_05_non_finite_brush_arguments_leave_plugin_and_undo_stack_unchanged() {
    let (mut document, path) = fixture("non-finite");
    let original = document.active_plugin().clone();
    for (center, radius, strength) in [
        ([f32::NAN, 2048.0], 256.0, 1.0),
        ([2048.0, 2048.0], f32::INFINITY, 1.0),
        ([2048.0, 2048.0], 256.0, f32::NAN),
    ] {
        assert!(document
            .brush_land_heights(LAND_FORM, center, radius, strength)
            .is_err());
        assert_eq!(document.active_plugin(), &original);
        assert!(!document.undo().unwrap());
    }
    let _ = fs::remove_file(path);
}
