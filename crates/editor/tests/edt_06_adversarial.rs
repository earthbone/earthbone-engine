// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::navm::{DoorLink, EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{GlobalFormId, Plugin, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-editor-edt-06-adversarial-{}-{label}.esp",
        std::process::id()
    ))
}

fn geometry() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 7,
        parent: Parent::Cell(0x123),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, 100.0, 0.0],
            [0.0, 100.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: Vec::new(),
        door_links: vec![DoorLink {
            triangle: 1,
            crc: 9,
            door: 0x456,
        }],
        cover_triangles: vec![1],
        grid: NavGrid {
            divisor: 2,
            size: [50.0, 50.0],
            min: [0.0, 0.0, 0.0],
            max: [100.0, 100.0, 0.0],
            cells: vec![vec![0, 1], vec![0], vec![1], vec![]],
        },
    }
}

fn document(label: &str, geometry: &Geometry) -> (EditorDocument, PathBuf) {
    let plugin_path = path(label);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x800).sub(b"NVNM", geometry.to_bytes()));
    fs::write(&plugin_path, plugin.to_bytes()).unwrap();
    let active_name = plugin_path.file_name().unwrap().to_string_lossy();
    let doc = EditorDocument::open(std::slice::from_ref(&plugin_path), &active_name).unwrap();
    (doc, plugin_path)
}

fn read_geometry(document: &EditorDocument) -> Geometry {
    let record = &document.active_plugin().records[0];
    Geometry::parse(Tag(*b"NAVM"), record.find(Tag(*b"NVNM")).unwrap()).unwrap()
}

fn full_triangle_geometry(count: usize) -> Geometry {
    let mut value = geometry();
    value.vertices.clear();
    value.triangles.clear();
    value.door_links.clear();
    value.cover_triangles.clear();
    value.vertices.push([0.0, 0.0, 0.0]);
    for index in 0..count {
        let angle = std::f32::consts::TAU * index as f32 / count as f32;
        value.vertices.push([angle.cos() * 100.0, angle.sin() * 100.0, 0.0]);
    }
    for index in 0..count.saturating_sub(1) {
        value.triangles.push(Triangle {
            vertices: [0, (index + 1) as u16, (index + 2) as u16],
            edges: [-1; 3],
            flags: 0,
            cover_flags: 0,
        });
    }
    value
}

#[test]
fn edt_06_last_signed_triangle_index_survives_plugin_reload() {
    let source = full_triangle_geometry(i16::MAX as usize + 1);
    let (mut doc, plugin_path) = document("last-signed-index", &source);

    doc.add_navmesh_triangle(FORM, [0, (i16::MAX as u16) + 1, 1], 0, 0)
        .unwrap();
    assert_eq!(read_geometry(&doc).triangles.len(), i16::MAX as usize + 1);

    doc.save(&plugin_path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&plugin_path).unwrap()).unwrap();
    let decoded = Geometry::parse(Tag(*b"NAVM"), reloaded.records[0].find(Tag(*b"NVNM")).unwrap())
        .unwrap();
    assert_eq!(decoded.triangles.len(), i16::MAX as usize + 1);
    assert!(decoded.grid.cells.iter().flatten().any(|index| *index == i16::MAX));
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_deletion_remaps_later_internal_links_and_triangle_metadata() {
    let mut source = geometry();
    source.triangles.push(Triangle {
        vertices: [0, 3, 1],
        edges: [-1, -1, -1],
        flags: 0,
        cover_flags: 0,
    });
    source.door_links.push(DoorLink {
        triangle: 2,
        crc: 10,
        door: 0x457,
    });
    source.cover_triangles.push(2);
    let (mut doc, plugin_path) = document("middle-delete", &source);

    doc.delete_navmesh_triangle(FORM, 1).unwrap();
    let changed = read_geometry(&doc);
    assert_eq!(changed.triangles.len(), 2);
    assert!(changed.triangles.iter().all(|triangle| {
        triangle.edges.iter().all(|edge| *edge == -1 || *edge >= 0 && (*edge as usize) < 2)
    }));
    assert_eq!(changed.door_links, [DoorLink { triangle: 1, crc: 10, door: 0x457 }]);
    assert_eq!(changed.cover_triangles, [1]);
    assert!(changed.grid.cells.iter().flatten().all(|index| (0..2).contains(index)));
    doc.save(&plugin_path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&plugin_path).unwrap()).unwrap();
    let round_trip =
        Geometry::parse(Tag(*b"NAVM"), reloaded.records[0].find(Tag(*b"NVNM")).unwrap())
            .unwrap();
    assert_eq!(round_trip, changed);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_external_edge_link_target_is_not_treated_as_local_triangle_index() {
    let mut source = geometry();
    source.edge_links.push(EdgeLink {
        kind: 1,
        navmesh: 0x900,
        triangle: 12,
    });
    source.triangles[0].flags |= 1;
    source.triangles[0].edges[0] = 0;
    let (mut doc, plugin_path) = document("remote-edge", &source);

    doc.delete_navmesh_triangle(FORM, 1).unwrap();
    let changed = read_geometry(&doc);
    assert_eq!(changed.triangles[0].edges[0], 0);
    assert_eq!(changed.triangles[0].flags & 1, 1);
    assert_eq!(changed.edge_links, source.edge_links);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_rejected_vertex_moves_leave_geometry_and_undo_stack_unchanged() {
    let (mut doc, plugin_path) = document("bad-moves", &geometry());
    let before = doc.active_plugin().clone();

    assert!(doc.move_navmesh_vertex(FORM, 1, [f32::NAN, 0.0, 0.0]).is_err());
    assert!(doc.move_navmesh_vertex(FORM, usize::MAX, [1.0, 1.0, 0.0]).is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_successive_edits_undo_in_reverse_to_exact_plugin_bytes() {
    let (mut doc, plugin_path) = document("multi-undo", &geometry());
    let initial = doc.active_plugin().to_bytes();

    doc.add_navmesh_triangle(FORM, [0, 1, 3], 0x0040, 0x0010)
        .unwrap();
    let after_add = doc.active_plugin().to_bytes();
    doc.move_navmesh_vertex(FORM, 3, [0.0, 110.0, 0.0]).unwrap();
    assert!(doc.undo().unwrap());
    assert_eq!(doc.active_plugin().to_bytes(), after_add);
    assert!(doc.undo().unwrap());
    assert_eq!(doc.active_plugin().to_bytes(), initial);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_missing_form_and_navmesh_without_geometry_are_atomic_errors() {
    let (mut doc, plugin_path) = document("missing-forms", &geometry());
    let before = doc.active_plugin().clone();
    let missing = GlobalFormId { file: 0, object: 0x999 };
    assert!(doc.add_navmesh_triangle(missing, [0, 1, 2], 0, 0).is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x800));
    fs::write(&plugin_path, plugin.to_bytes()).unwrap();
    let active_name = plugin_path.file_name().unwrap().to_string_lossy();
    let mut no_geometry =
        EditorDocument::open(std::slice::from_ref(&plugin_path), &active_name).unwrap();
    let before = no_geometry.active_plugin().clone();
    assert!(no_geometry.add_navmesh_triangle(FORM, [0, 1, 2], 0, 0).is_err());
    assert_eq!(no_geometry.active_plugin(), &before);
    assert!(!no_geometry.undo().unwrap());
    let _ = fs::remove_file(plugin_path);
}
