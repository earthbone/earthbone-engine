// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::Land;
use soe_esm::{GlobalFormId, Plugin, Tag};

const LAND_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const PAINTED_TEXTURE: u32 = 0x0000_1234;
const PAINTED_TEXTURE_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: PAINTED_TEXTURE,
};
const OTHER_TEXTURE: u32 = 0x0000_5678;
const BASE_TEXTURE: u32 = 0x0000_7777;
const ALPHA_POSITION: u16 = 8 * 17 + 8;

fn alpha(position: u16, opacity: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&position.to_le_bytes());
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(&opacity.to_le_bytes());
    bytes
}

fn layer_header(texture: u32, quadrant: u8, index: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&texture.to_le_bytes());
    bytes.push(quadrant);
    bytes.push(0);
    bytes.extend_from_slice(&index.to_le_bytes());
    bytes
}

fn fixture(label: &str) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-editor-edt-26-{}-{label}.esp",
        std::process::id()
    ));
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"LAND"), 0x800)
            .sub(Tag(*b"DATA"), 4u32.to_le_bytes())
            .sub(Tag(*b"BTXT"), layer_header(BASE_TEXTURE, 0, 0))
            .sub(Tag(*b"ATXT"), layer_header(PAINTED_TEXTURE, 0, 2))
            .sub(Tag(*b"VTXT"), alpha(ALPHA_POSITION, 0.25))
            .sub(Tag(*b"ATXT"), layer_header(OTHER_TEXTURE, 0, 4))
            .sub(Tag(*b"VTXT"), alpha(ALPHA_POSITION, 0.5))
            .sub(Tag(*b"ATXT"), layer_header(0x0000_9abc, 1, 1))
            .sub(Tag(*b"VTXT"), alpha(ALPHA_POSITION, 0.75))
            .sub(Tag(*b"UNKN"), [7, 8, 9]),
    );
    let _ = fs::remove_file(&path);
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active_name = path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(std::slice::from_ref(&path), &active_name).unwrap();
    (document, path)
}

fn paint(document: &mut EditorDocument) {
    document
        .brush_land_texture(
            LAND_FORM,
            PAINTED_TEXTURE_FORM,
            0,
            [1024.0, 1024.0],
            128.0,
            0.5,
        )
        .unwrap();
}

#[test]
fn edt_26_texture_brush_preserves_layer_identities_and_other_alpha_weights() {
    let (mut document, path) = fixture("stroke");
    paint(&mut document);

    let record = document.load_order().winner(LAND_FORM).unwrap();
    let land = Land::from_record(record).unwrap();
    assert_eq!(land.quadrants[0].base, Some(BASE_TEXTURE));
    assert_eq!(land.quadrants[0].layers.len(), 2);
    assert_eq!(land.quadrants[0].layers[0].texture, PAINTED_TEXTURE);
    assert_eq!(land.quadrants[0].layers[0].index, 2);
    assert_eq!(
        land.quadrants[0].layers[0].alphas[0].position,
        ALPHA_POSITION
    );
    assert_eq!(land.quadrants[0].layers[0].alphas[0].opacity, 0.75);
    assert_eq!(land.quadrants[0].layers[1].texture, OTHER_TEXTURE);
    assert_eq!(land.quadrants[0].layers[1].index, 4);
    assert_eq!(land.quadrants[0].layers[1].alphas[0].opacity, 0.5);
    assert_eq!(land.quadrants[1].layers[0].texture, 0x0000_9abc);
    assert_eq!(land.quadrants[1].layers[0].alphas[0].opacity, 0.75);
    assert_eq!(record.find(Tag(*b"UNKN")), Some(&[7, 8, 9][..]));
    let _ = fs::remove_file(path);
}

#[test]
fn edt_26_texture_brush_undo_restores_layer_headers_and_alpha_bytes() {
    let (mut document, path) = fixture("undo");
    let original = document.active_plugin().to_bytes();
    paint(&mut document);

    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin().to_bytes(), original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn edt_26_texture_brush_plugin_save_reload_preserves_ids_and_weights() {
    let (mut document, path) = fixture("save");
    paint(&mut document);
    let saved = document.save(&path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&path).unwrap()).unwrap();

    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    let land = Land::from_record(reloaded.records_of(Tag(*b"LAND")).next().unwrap()).unwrap();
    assert_eq!(land.quadrants[0].layers[0].texture, PAINTED_TEXTURE);
    assert_eq!(land.quadrants[0].layers[0].index, 2);
    assert_eq!(land.quadrants[0].layers[0].alphas[0].opacity, 0.75);
    assert_eq!(land.quadrants[0].layers[1].texture, OTHER_TEXTURE);
    assert_eq!(land.quadrants[0].layers[1].alphas[0].opacity, 0.5);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_26_texture_brush_creates_missing_layer_in_first_unused_slot() {
    let (mut document, path) = fixture("new-layer");
    let texture = GlobalFormId {
        file: 0,
        object: 0x0000_cdef,
    };
    document
        .brush_land_texture(LAND_FORM, texture, 0, [1024.0, 1024.0], 128.0, 0.5)
        .unwrap();

    let record = document.load_order().winner(LAND_FORM).unwrap();
    let land = Land::from_record(record).unwrap();
    let new_layer = land.quadrants[0]
        .layers
        .iter()
        .find(|layer| layer.texture == texture.object)
        .unwrap();
    assert_eq!(new_layer.index, 1);
    assert_eq!(new_layer.alphas[0].opacity, 0.5);
    assert_eq!(land.quadrants[0].layers[0].index, 2);
    assert_eq!(land.quadrants[0].layers[1].index, 4);
    let _ = fs::remove_file(path);
}
