// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::navm::{DoorLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{GlobalFormId, Plugin, Tag};
use soe_navmesh::{Agent, Scene};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn geometry() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 23,
        parent: Parent::Cell(0x123),
        vertices: vec![
            [3000.0, 3000.0, 0.0],
            [3100.0, 3000.0, 0.0],
            [3100.0, 3100.0, 0.0],
        ],
        triangles: vec![Triangle {
            vertices: [0, 1, 2],
            edges: [-1, -1, -1],
            flags: 0,
            cover_flags: 0x20,
        }],
        edge_links: Vec::new(),
        door_links: vec![DoorLink {
            triangle: 0,
            crc: 0xabcdef,
            door: 0x789,
        }],
        cover_triangles: vec![0],
        grid: NavGrid {
            divisor: 1,
            size: [100.0, 100.0],
            min: [3000.0, 3000.0, 0.0],
            max: [3100.0, 3100.0, 0.0],
            cells: vec![vec![0]],
        },
    }
}

fn document_with_geometry(
    label: &str,
    geometry: &Geometry,
) -> (EditorDocument, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!(
        "soe-editor-edt-28-breaker-{}-{label}.esp",
        std::process::id()
    ));
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NAVM"), FORM.object).sub(b"NVNM", geometry.to_bytes()));
    fs::write(&path, plugin.to_bytes()).unwrap();
    let active = path.file_name().unwrap().to_string_lossy();
    let doc = EditorDocument::open(std::slice::from_ref(&path), &active).unwrap();
    (doc, path)
}

#[test]
fn edt_28_saved_navmesh_and_authored_links_survive_plugin_reload() {
    let (mut doc, path) = document_with_geometry("reload", &geometry());
    let original = Geometry::parse(
        Tag(*b"NAVM"),
        doc.active_plugin().records[0].find(Tag(*b"NVNM")).unwrap(),
    )
    .unwrap();
    assert_eq!(original.door_links.len(), 1);
    let mut scene = Scene::new();
    scene.add_box([0.0, 0.0, -16.0], [1024.0, 1024.0, 0.0]);
    scene.add_box([400.0, 400.0, 0.0], [560.0, 560.0, 200.0]);

    doc.save_with_navmesh_regeneration(&path, FORM, &scene, &Agent::default(), &[0])
        .unwrap();

    let reloaded = Plugin::parse(&fs::read(&path).unwrap()).unwrap();
    let geometry = Geometry::parse(
        Tag(*b"NAVM"),
        reloaded.records[0].find(Tag(*b"NVNM")).unwrap(),
    )
    .unwrap();
    assert!(geometry
        .triangles
        .iter()
        .any(|triangle| triangle.cover_flags == 0x20));
    assert_eq!(geometry.door_links.len(), 1);
    let door_triangle = usize::try_from(geometry.door_links[0].triangle).unwrap();
    assert_eq!(geometry.triangles[door_triangle].cover_flags, 0x20);
    assert_eq!(geometry.door_links[0].crc, 0xabcdef);
    let _ = fs::remove_file(path);
}

#[test]
fn edt_28_invalid_authored_door_link_is_rejected_without_data_loss() {
    let mut geometry = geometry();
    geometry.door_links = vec![DoorLink {
        triangle: 99,
        crc: 0xabcdef,
        door: 0x789,
    }];
    let (mut doc, path) = document_with_geometry("invalid-link", &geometry);
    let before = doc.active_plugin().clone();
    let mut scene = Scene::new();
    scene.add_box([0.0, 0.0, -16.0], [1024.0, 1024.0, 0.0]);

    let result = doc.regenerate_navmesh(FORM, &scene, &Agent::default(), &[]);

    assert!(
        result.is_err(),
        "an invalid authored link must not be silently removed"
    );
    assert_eq!(doc.active_plugin(), &before);
    let _ = fs::remove_file(path);
}
