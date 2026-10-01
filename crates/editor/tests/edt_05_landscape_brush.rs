// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::{Heights, Land, VERTS, VERTS_PER_SIDE};
use soe_esm::{GlobalFormId, Plugin, Tag};

const LAND: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn fixture(label: &str) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-editor-edt-05-{}-{label}.esp",
        std::process::id()
    ));
    let heights = vec![0.0; VERTS];
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"LAND"), 0x800)
            .sub(Tag(*b"DATA"), 1u32.to_le_bytes())
            .sub(Tag(*b"VHGT"), Heights::encode(&heights).to_bytes())
            .sub(Tag(*b"UNKN"), [7, 8, 9]),
    );
    let _ = fs::remove_file(&path);
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active_name = path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(std::slice::from_ref(&path), &active_name).unwrap();
    (document, path)
}

#[test]
fn edt_05_height_brush_updates_land_heights_and_derived_normals() {
    let (mut document, path) = fixture("stroke");
    document
        .brush_land_heights(LAND, [2048.0, 2048.0], 256.0, 200.0)
        .unwrap();

    let record = document.load_order().winner(LAND).unwrap();
    let land = Land::from_record(record).unwrap();
    let heights = land.heights.unwrap().decode();
    let center = 16 * VERTS_PER_SIDE + 16;
    let adjacent = center + 1;
    assert_eq!(heights[center], 200.0);
    assert!(heights[adjacent] > 0.0);
    assert_eq!(land.normals.as_ref().unwrap().len(), VERTS);
    assert_ne!(land.normals.unwrap()[adjacent], [0, 0, 127]);
    assert_eq!(record.find(Tag(*b"UNKN")), Some(&[7, 8, 9][..]));
    let _ = fs::remove_file(path);
}

#[test]
fn edt_05_height_brush_is_undone_as_one_record_edit() {
    let (mut document, path) = fixture("undo");
    let original = document.active_plugin().clone();
    document
        .brush_land_heights(LAND, [2048.0, 2048.0], 256.0, 200.0)
        .unwrap();
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn edt_05_height_brush_survives_plugin_save_and_reload() {
    let (mut document, path) = fixture("save");
    document
        .brush_land_heights(LAND, [2048.0, 2048.0], 256.0, 200.0)
        .unwrap();
    let saved = document.save(&path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&path).unwrap()).unwrap();

    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    let land = Land::from_record(reloaded.records_of(Tag(*b"LAND")).next().unwrap()).unwrap();
    assert_eq!(
        land.heights.unwrap().decode()[16 * VERTS_PER_SIDE + 16],
        200.0
    );
    assert_ne!(land.normals.unwrap()[16 * VERTS_PER_SIDE + 17], [0, 0, 127]);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_05_height_brush_rejects_unrepresentable_slope_without_editing() {
    let (mut document, path) = fixture("slope-limit");
    let original = document.active_plugin().clone();

    assert!(document
        .brush_land_heights(LAND, [2048.0, 2048.0], 128.0, 2_000.0)
        .is_err());
    assert_eq!(document.active_plugin(), &original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(path);
}
