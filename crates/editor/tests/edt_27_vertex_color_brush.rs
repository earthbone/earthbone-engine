// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::land::{Land, VERTS};
use soe_esm::{GlobalFormId, Plugin, Tag};

const SELECTED: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const OTHER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x801,
};
const ORIGINAL: [u8; 3] = [12, 34, 56];
const PAINT: [u8; 3] = [200, 100, 50];

fn fixture(label: &str) -> (EditorDocument, PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-editor-edt-27-{}-{label}.esp",
        std::process::id()
    ));
    let colours = vec![ORIGINAL; VERTS]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let land_record = |id| {
        Record::new(Tag(*b"LAND"), id)
            .sub(Tag(*b"DATA"), 0u32.to_le_bytes())
            .sub(Tag(*b"VCLR"), colours.clone())
            .sub(Tag(*b"UNKN"), [7, 8, 9])
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(land_record(0x800));
    plugin.push_top(land_record(0x801));
    let _ = fs::remove_file(&path);
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active_name = path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(std::slice::from_ref(&path), &active_name).unwrap();
    (document, path)
}

#[test]
fn edt_27_vertex_color_brush_changes_only_selected_land_vertices() {
    let (mut document, path) = fixture("stroke");
    document
        .brush_land_vertex_colors(SELECTED, [2048.0, 2048.0], 256.0, PAINT, 1.0)
        .unwrap();

    let selected = Land::from_record(document.load_order().winner(SELECTED).unwrap()).unwrap();
    let other = Land::from_record(document.load_order().winner(OTHER).unwrap()).unwrap();
    let selected_colours = selected.colours.unwrap();
    let center = 16 * 33 + 16;
    assert_eq!(selected_colours[center], PAINT);
    assert_ne!(selected_colours[center + 1], ORIGINAL);
    assert_ne!(selected_colours[center + 1], PAINT);
    assert_eq!(selected_colours[center + 2], ORIGINAL);
    assert!(selected.flags & soe_esm::records::land::flags::HAS_VERTEX_COLOURS != 0);
    assert!(other
        .colours
        .unwrap()
        .iter()
        .all(|colour| *colour == ORIGINAL));
    assert_eq!(
        document
            .load_order()
            .winner(SELECTED)
            .unwrap()
            .find(Tag(*b"UNKN")),
        Some(&[7, 8, 9][..])
    );
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_vertex_color_brush_is_undone_as_one_record_edit() {
    let (mut document, path) = fixture("undo");
    let original = document.active_plugin().clone();
    document
        .brush_land_vertex_colors(SELECTED, [2048.0, 2048.0], 256.0, PAINT, 1.0)
        .unwrap();

    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_vertex_color_brush_survives_plugin_save_and_reload() {
    let (mut document, path) = fixture("save");
    document
        .brush_land_vertex_colors(SELECTED, [2048.0, 2048.0], 256.0, PAINT, 1.0)
        .unwrap();

    let saved = document.save(&path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    let land = Land::from_record(reloaded.records_of(Tag(*b"LAND")).next().unwrap()).unwrap();
    assert_eq!(land.colours.unwrap()[16 * 33 + 16], PAINT);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_27_vertex_color_brush_rejects_invalid_strength_without_editing() {
    let (mut document, path) = fixture("invalid-strength");
    let original = document.active_plugin().clone();
    assert!(document
        .brush_land_vertex_colors(SELECTED, [2048.0, 2048.0], 256.0, PAINT, 1.1)
        .is_err());
    assert_eq!(document.active_plugin(), &original);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(path);
}
