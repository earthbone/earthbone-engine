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
        "soe-editor-edt-06-{}-{label}.esp",
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

fn document(label: &str) -> (EditorDocument, PathBuf) {
    document_with_geometry(label, &geometry())
}

fn document_with_geometry(label: &str, geometry: &Geometry) -> (EditorDocument, PathBuf) {
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
    Geometry::parse(record.tag, record.find(Tag(*b"NVNM")).unwrap()).unwrap()
}

#[test]
fn edt_06_triangle_addition_rebuilds_adjacency_and_grid() {
    let (mut doc, plugin_path) = document("add");
    doc.add_navmesh_triangle(FORM, [0, 1, 3], 0, 0).unwrap();
    let geometry = read_geometry(&doc);
    assert_eq!(geometry.triangles.len(), 3);
    assert_eq!(geometry.triangles[0].edges[0], 2);
    assert_eq!(geometry.triangles[2].edges[0], 0);
    assert!(geometry
        .grid
        .cells
        .iter()
        .flatten()
        .any(|index| *index == 2));
    assert!(doc.undo().unwrap());
    assert_eq!(read_geometry(&doc).triangles.len(), 2);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_triangle_deletion_remaps_triangle_metadata() {
    let (mut doc, plugin_path) = document("delete");
    doc.delete_navmesh_triangle(FORM, 1).unwrap();
    let geometry = read_geometry(&doc);
    assert_eq!(geometry.triangles.len(), 1);
    assert_eq!(geometry.triangles[0].edges[2], -1);
    assert!(geometry.door_links.is_empty());
    assert!(geometry.cover_triangles.is_empty());
    assert!(geometry
        .grid
        .cells
        .iter()
        .flatten()
        .all(|index| *index == 0));
    assert!(doc.undo().unwrap());
    assert_eq!(read_geometry(&doc).triangles.len(), 2);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_triangle_deletion_preserves_external_edge_link_indices() {
    let mut geometry = geometry();
    geometry.edge_links.push(EdgeLink {
        kind: 1,
        navmesh: 0x900,
        triangle: 12,
    });
    geometry.triangles[0].flags |= 1;
    geometry.triangles[0].edges[0] = 0;
    let (mut doc, plugin_path) = document_with_geometry("edge-link", &geometry);

    doc.delete_navmesh_triangle(FORM, 1).unwrap();
    let edited = read_geometry(&doc);
    assert_eq!(edited.triangles[0].edges[0], 0);
    assert_eq!(edited.triangles[0].flags & 1, 1);
    assert_eq!(edited.edge_links, geometry.edge_links);
    assert!(doc.undo().unwrap());
    assert_eq!(read_geometry(&doc).triangles.len(), 2);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_vertex_movement_updates_coordinates_and_bounds() {
    let (mut doc, plugin_path) = document("move");
    doc.move_navmesh_vertex(FORM, 2, [180.0, 120.0, 8.0])
        .unwrap();
    let geometry = read_geometry(&doc);
    assert_eq!(geometry.vertices[2], [180.0, 120.0, 8.0]);
    assert_eq!(geometry.grid.max, [180.0, 120.0, 8.0]);
    assert_eq!(geometry.triangles[0].edges[2], 1);
    assert!(doc.undo().unwrap());
    assert_eq!(read_geometry(&doc).vertices[2], [100.0, 100.0, 0.0]);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_plugin_save_reload_round_trips_edited_navmesh() {
    let (mut doc, plugin_path) = document("save");
    doc.add_navmesh_triangle(FORM, [0, 1, 3], 0x0040, 0x0010)
        .unwrap();
    doc.move_navmesh_vertex(FORM, 3, [0.0, 110.0, 0.0]).unwrap();
    let saved = doc.save(&plugin_path).unwrap();
    let reloaded = Plugin::parse(&fs::read(&plugin_path).unwrap()).unwrap();
    let before =
        Geometry::parse(Tag(*b"NAVM"), saved.records[0].find(Tag(*b"NVNM")).unwrap()).unwrap();
    let after = Geometry::parse(
        Tag(*b"NAVM"),
        reloaded.records[0].find(Tag(*b"NVNM")).unwrap(),
    )
    .unwrap();
    assert_eq!(before, after);
    assert_eq!(after.triangles.len(), 3);
    assert_eq!(after.triangles[2].flags, 0x0040);
    assert_eq!(after.triangles[2].cover_flags, 0x0010);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_invalid_triangle_edit_is_rejected_without_mutation() {
    let (mut doc, plugin_path) = document("invalid");
    let before = doc.active_plugin().clone();
    assert!(doc.add_navmesh_triangle(FORM, [0, 0, 2], 0, 0).is_err());
    assert_eq!(doc.active_plugin(), &before);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_06_invalid_vertex_movement_is_rejected_without_mutation() {
    let (mut doc, plugin_path) = document("invalid-move");
    let before = doc.active_plugin().clone();
    assert!(doc.move_navmesh_vertex(FORM, 2, [0.0, 0.0, 0.0]).is_err());
    assert_eq!(doc.active_plugin(), &before);
    assert!(!doc.undo().unwrap());
    let _ = fs::remove_file(plugin_path);
}
