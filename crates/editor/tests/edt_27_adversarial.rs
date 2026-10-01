// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::{Land, VERTS};
use soe_esm::{GlobalFormId, Plugin, Tag};

const LAND_FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const ORIGINAL: [u8; 3] = [12, 34, 56];
const PAINT: [u8; 3] = [200, 100, 50];
const CENTER: [f32; 2] = [2048.0, 2048.0];
const CENTER_INDEX: usize = 16 * 33 + 16;

fn fixture(label: &str, with_colours: bool) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-edt-27-breaker-{}-{label}.esp",
        std::process::id()
    ));
    let mut record = Record::new(Tag(*b"LAND"), 0x800).sub(Tag(*b"DATA"), 0x8000u32.to_le_bytes());
    if with_colours {
        let bytes = vec![ORIGINAL; VERTS]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        record = record.sub(Tag(*b"VCLR"), bytes);
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
fn edt_27_missing_vclr_paints_from_white_and_preserves_unrelated_land_flags() {
    let (mut doc, path) = fixture("missing-vclr", false);
    doc.brush_land_vertex_colors(LAND_FORM, CENTER, 256.0, PAINT, 0.5)
        .unwrap();
    let land = Land::from_record(doc.load_order().winner(LAND_FORM).unwrap()).unwrap();
    let colors = land.colours.unwrap();
    assert_eq!(colors[CENTER_INDEX], [228, 178, 153]);
    assert!(land.flags & 0x8000 != 0);
    assert!(land.flags & soe_esm::records::land::flags::HAS_VERTEX_COLOURS != 0);
    assert_eq!(colors[0], [255; 3]);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_repeated_strokes_blend_selected_color_again_without_touching_neighbors() {
    let (mut doc, path) = fixture("repeat", true);
    for _ in 0..2 {
        doc.brush_land_vertex_colors(LAND_FORM, CENTER, 256.0, PAINT, 0.5)
            .unwrap();
    }
    let land = Land::from_record(doc.load_order().winner(LAND_FORM).unwrap()).unwrap();
    let colors = land.colours.unwrap();
    assert_eq!(colors[CENTER_INDEX], [153, 84, 52]);
    assert_eq!(colors[CENTER_INDEX + 2], ORIGINAL);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_zero_strength_leaves_effective_white_colors_unchanged() {
    let (mut doc, path) = fixture("zero-strength", false);
    doc.brush_land_vertex_colors(LAND_FORM, CENTER, 256.0, PAINT, 0.0)
        .unwrap();
    let land = Land::from_record(doc.load_order().winner(LAND_FORM).unwrap()).unwrap();
    assert!(land.colours.unwrap().iter().all(|color| *color == [255; 3]));
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_missing_land_rejects_stroke_without_mutating_plugin() {
    let (mut doc, path) = fixture("missing-land", true);
    let before = doc.active_plugin().clone();
    let missing = GlobalFormId {
        file: 0,
        object: 0x999,
    };
    assert!(doc
        .brush_land_vertex_colors(missing, CENTER, 256.0, PAINT, 1.0)
        .is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(path);
}
