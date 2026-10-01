// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::navm::{DoorLink, EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{GlobalFormId, Plugin, Tag};
use soe_navmesh::{Agent, Scene};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "soe-editor-edt-28-{}-{label}.esp",
        std::process::id()
    ))
}

fn geometry() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 23,
        parent: Parent::Cell(0x123),
        vertices: vec![
            [3000.0, 3000.0, 0.0],
            [3100.0, 3000.0, 0.0],
            [3100.0, 3100.0, 0.0],
            [3000.0, 3100.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, -1],
                flags: 0,
                cover_flags: 0x20,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 1,
                cover_flags: 0x40,
            },
        ],
        edge_links: vec![EdgeLink {
            kind: 0,
            navmesh: 0x456,
            triangle: 7,
        }],
        door_links: vec![DoorLink {
            triangle: 1,
            crc: 0xabcdef,
            door: 0x789,
        }],
        cover_triangles: vec![0, 1],
        grid: NavGrid {
            divisor: 2,
            size: [50.0, 50.0],
            min: [3000.0, 3000.0, 0.0],
            max: [3100.0, 3100.0, 0.0],
            cells: vec![vec![0, 1], vec![0], vec![1], vec![]],
        },
    }
}

fn document(label: &str) -> (EditorDocument, PathBuf) {
    let plugin_path = path(label);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x800).sub(b"NVNM", geometry().to_bytes()));
    fs::write(&plugin_path, plugin.to_bytes()).unwrap();
    let active_name = plugin_path.file_name().unwrap().to_string_lossy();
    let doc = EditorDocument::open(std::slice::from_ref(&plugin_path), &active_name).unwrap();
    (doc, plugin_path)
}

fn read_geometry(document: &EditorDocument) -> Geometry {
    let record = &document.active_plugin().records[0];
    Geometry::parse(Tag(*b"NAVM"), record.find(Tag(*b"NVNM")).unwrap()).unwrap()
}

fn changed_collision_scene() -> Scene {
    let mut scene = Scene::new();
    scene.add_box([0.0, 0.0, -16.0], [1024.0, 1024.0, 0.0]);
    scene.add_box([400.0, 400.0, 0.0], [560.0, 560.0, 200.0]);
    scene
}

#[test]
fn edt_28_changed_collision_regenerates_unlocked_triangles() {
    let (mut doc, plugin_path) = document("unlocked");
    let original = read_geometry(&doc);
    let agent = Agent::default();
    let saved = doc
        .save_with_navmesh_regeneration(
            &plugin_path,
            FORM,
            &changed_collision_scene(),
            &agent,
            &[0],
        )
        .unwrap();
    let regenerated = read_geometry(&doc);
    assert!(regenerated.triangles.len() > original.triangles.len());
    assert!(regenerated.triangles.iter().any(|triangle| triangle
        .vertices
        .iter()
        .any(|index| { regenerated.vertices[usize::from(*index)][2] > 0.0 })));
    let saved_geometry =
        Geometry::parse(Tag(*b"NAVM"), saved.records[0].find(Tag(*b"NVNM")).unwrap()).unwrap();
    assert_eq!(saved_geometry, regenerated);
    assert!(doc.undo().unwrap());
    assert_eq!(read_geometry(&doc), original);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_28_locked_triangles_retain_authored_geometry() {
    let (mut doc, plugin_path) = document("locked");
    let locked = geometry().triangles[0];
    let locked_points = locked
        .vertices
        .map(|index| geometry().vertices[usize::from(index)]);
    let agent = Agent::default();
    let saved = doc
        .save_with_navmesh_regeneration(
            &plugin_path,
            FORM,
            &changed_collision_scene(),
            &agent,
            &[0],
        )
        .unwrap();
    let regenerated = read_geometry(&doc);
    let retained = regenerated
        .triangles
        .iter()
        .find(|triangle| triangle.cover_flags == locked.cover_flags)
        .unwrap();
    assert_eq!(
        retained
            .vertices
            .map(|index| regenerated.vertices[usize::from(index)]),
        locked_points
    );
    let saved_geometry =
        Geometry::parse(Tag(*b"NAVM"), saved.records[0].find(Tag(*b"NVNM")).unwrap()).unwrap();
    assert_eq!(saved_geometry, regenerated);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_28_authored_links_retain_their_triangle_geometry() {
    let (mut doc, plugin_path) = document("links");
    let agent = Agent::default();
    let saved = doc
        .save_with_navmesh_regeneration(&plugin_path, FORM, &changed_collision_scene(), &agent, &[])
        .unwrap();
    let regenerated = read_geometry(&doc);
    assert_eq!(regenerated.edge_links, geometry().edge_links);
    assert_eq!(regenerated.door_links.len(), 1);
    assert_eq!(regenerated.door_links[0].crc, 0xabcdef);
    let linked_index = usize::try_from(regenerated.door_links[0].triangle).unwrap();
    let linked = &regenerated.triangles[linked_index];
    assert_eq!(linked.flags & 1, 1);
    assert_eq!(linked.edges[0], 0);
    assert!(regenerated
        .cover_triangles
        .contains(&i16::try_from(linked_index).unwrap()));
    let saved_geometry =
        Geometry::parse(Tag(*b"NAVM"), saved.records[0].find(Tag(*b"NVNM")).unwrap()).unwrap();
    assert_eq!(saved_geometry.edge_links, regenerated.edge_links);
    assert_eq!(saved_geometry.door_links, regenerated.door_links);
    let _ = fs::remove_file(plugin_path);
}

#[test]
fn edt_28_invalid_locked_triangle_rejects_without_mutation() {
    let (mut doc, plugin_path) = document("invalid");
    let before = doc.active_plugin().clone();
    let error = doc
        .regenerate_navmesh(FORM, &changed_collision_scene(), &Agent::default(), &[2])
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("locked triangle index is out of range"));
    assert_eq!(doc.active_plugin(), &before);
    let _ = fs::remove_file(plugin_path);
}
