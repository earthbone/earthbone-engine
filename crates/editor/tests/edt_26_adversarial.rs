// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::Land;
use soe_esm::{GlobalFormId, Plugin, Tag};

const LAND_FORM: GlobalFormId = GlobalFormId { file: 0, object: 0x800 };
const TEXTURE: GlobalFormId = GlobalFormId { file: 0, object: 0x1234 };
const BASE: u32 = 0x7777;
const ALPHA_POS: u16 = 8 * 17 + 8;

fn header(texture: u32, quadrant: u8, index: u16) -> Vec<u8> {
    let mut bytes = texture.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[quadrant, 0]);
    bytes.extend_from_slice(&index.to_le_bytes());
    bytes
}

fn vtxt(position: u16, weight: f32) -> Vec<u8> {
    let mut bytes = position.to_le_bytes().to_vec();
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&weight.to_le_bytes());
    bytes
}

fn fixture(label: &str, duplicate_target: bool) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!("soe-edt-26-breaker-{}-{label}.esp", std::process::id()));
    let mut record = Record::new(Tag(*b"LAND"), 0x800)
        .sub(Tag(*b"DATA"), 4u32.to_le_bytes())
        .sub(Tag(*b"BTXT"), header(BASE, 0, 0))
        .sub(Tag(*b"ATXT"), header(TEXTURE.object, 0, 2))
        .sub(Tag(*b"VTXT"), vtxt(ALPHA_POS, 0.25));
    if duplicate_target {
        record = record
            .sub(Tag(*b"ATXT"), header(TEXTURE.object, 0, 5))
            .sub(Tag(*b"VTXT"), vtxt(ALPHA_POS, 0.5));
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let _ = fs::remove_file(&path);
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active = path.file_name().unwrap().to_string_lossy();
    let doc = EditorDocument::open(std::slice::from_ref(&path), &active).unwrap();
    (doc, path)
}

#[test]
fn edt_26_repeated_strokes_accumulate_one_alpha_without_changing_layer_identity() {
    let (mut doc, path) = fixture("repeat", false);
    for _ in 0..3 {
        doc.brush_land_texture(LAND_FORM, TEXTURE, 0, [1024.0, 1024.0], 128.0, 0.5)
            .unwrap();
    }
    let land = Land::from_record(doc.load_order().winner(LAND_FORM).unwrap()).unwrap();
    let layer = &land.quadrants[0].layers[0];
    assert_eq!((layer.texture, layer.index), (TEXTURE.object, 2));
    assert_eq!(layer.alphas.len(), 1);
    assert_eq!(layer.alphas[0].position, ALPHA_POS);
    assert_eq!(layer.alphas[0].opacity, 1.0);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_26_duplicate_target_layer_rejects_stroke_without_creating_undo_state() {
    let (mut doc, path) = fixture("duplicate", true);
    let before = doc.active_plugin().clone();
    assert!(doc
        .brush_land_texture(LAND_FORM, TEXTURE, 0, [1024.0, 1024.0], 128.0, 0.5)
        .is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn edt_26_missing_land_rejects_stroke_without_mutating_plugin() {
    let (mut doc, path) = fixture("missing", false);
    let before = doc.active_plugin().clone();
    let missing = GlobalFormId { file: 0, object: 0x999 };
    assert!(doc
        .brush_land_texture(missing, TEXTURE, 0, [1024.0, 1024.0], 128.0, 0.5)
        .is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(path);
}
