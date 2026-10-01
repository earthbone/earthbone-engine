// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{GlobalFormId, Plugin, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-editor-edt-06-breaker-{}-{label}.esp",
        std::process::id()
    ))
}

fn geometry(triangle_count: usize) -> Geometry {
    let mut vertices = vec![[0.0, 0.0, 0.0]];
    for index in 0..=triangle_count {
        let angle = std::f32::consts::TAU * index as f32 / (triangle_count + 1) as f32;
        vertices.push([angle.cos() * 100.0, angle.sin() * 100.0, 0.0]);
    }
    let triangles = (0..triangle_count)
        .map(|index| Triangle {
            vertices: [0, (index + 1) as u16, (index + 2) as u16],
            edges: [-1; 3],
            flags: 0,
            cover_flags: 0,
        })
        .collect();
    Geometry {
        version: 12,
        location_crc: 7,
        parent: Parent::Cell(0x123),
        vertices,
        triangles,
        edge_links: Vec::new(),
        door_links: Vec::new(),
        cover_triangles: Vec::new(),
        grid: NavGrid {
            divisor: 1,
            size: [200.0, 200.0],
            min: [-100.0, -100.0, 0.0],
            max: [100.0, 100.0, 0.0],
            cells: vec![Vec::new()],
        },
    }
}

fn document(label: &str, geometry: &Geometry) -> (EditorDocument, PathBuf) {
    let plugin_path = path(label);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x800).sub(b"NVNM", geometry.to_bytes()));
    fs::write(&plugin_path, plugin.to_bytes()).unwrap();
    let active_name = plugin_path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(std::slice::from_ref(&plugin_path), &active_name).unwrap();
    (document, plugin_path)
}

fn read_geometry(document: &EditorDocument) -> Geometry {
    Geometry::parse(
        Tag(*b"NAVM"),
        document.active_plugin().records[0]
            .find(Tag(*b"NVNM"))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn edt_06_addition_beyond_signed_triangle_capacity_is_rejected_atomically() {
    let source = geometry(i16::MAX as usize + 1);
    let (mut document, plugin_path) = document("over-capacity", &source);
    let before = document.active_plugin().clone();

    assert!(document
        .add_navmesh_triangle(FORM, [0, 1, 2], 0, 0)
        .is_err());
    assert_eq!(document.active_plugin(), &before);
    assert_eq!(read_geometry(&document).triangles.len(), i16::MAX as usize + 1);
    assert!(!document.undo().unwrap());
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_deletion_at_each_end_preserves_valid_signed_references_after_reload() {
    let source = geometry(4);
    let (mut document, plugin_path) = document("end-deletions", &source);

    document.delete_navmesh_triangle(FORM, 0).unwrap();
    document.delete_navmesh_triangle(FORM, 2).unwrap();
    let changed = read_geometry(&document);
    assert_eq!(changed.triangles.len(), 2);
    assert!(changed
        .grid
        .cells
        .iter()
        .flatten()
        .all(|index| (0..2).contains(index)));
    assert!(changed.triangles.iter().all(|triangle| triangle
        .edges
        .iter()
        .all(|edge| *edge == -1 || (0..2).contains(&(*edge as usize)))));

    document.save(&plugin_path).unwrap();
    let saved = Plugin::parse(&fs::read(&plugin_path).unwrap()).unwrap();
    let round_trip = Geometry::parse(
        Tag(*b"NAVM"),
        saved.records[0].find(Tag(*b"NVNM")).unwrap(),
    )
    .unwrap();
    assert_eq!(round_trip, changed);
    let _ = fs::remove_file(plugin_path);
}
