// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Two navmeshes linked across a shared edge, then a patch that deletes one
//! of them the way the shipped masters delete navmeshes. The graph drops the
//! deleted one and cuts the neighbour's link to it, counted as repaired.

use soe_esm::records::navm::{EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{plugin::flags, tag, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record};
use soe_navmesh::vanilla;
use soe_navmesh::NodeId;

const WORLD: u32 = 0x100;
const EXT: u32 = 0x201;
const NAVM_1: u32 = 0x501;
const NAVM_2: u32 = 0x502;

/// A square of two triangles at `origin` with an optional link out of
/// triangle 0's right side, the same shape as the crate's unit tests use.
fn square(origin: f32, link_to: Option<(u32, i16)>) -> Geometry {
    let o = origin;
    let mut t0 = Triangle {
        vertices: [0, 1, 2],
        edges: [-1, -1, 1],
        flags: 0,
        cover_flags: 0,
    };
    let t1 = Triangle {
        vertices: [0, 2, 3],
        edges: [0, -1, -1],
        flags: 0,
        cover_flags: 0,
    };
    let mut edge_links = Vec::new();
    if let Some((mesh, tri)) = link_to {
        t0.edges[1] = 0;
        t0.flags |= vanilla::flags::EDGE_1_2_LINK;
        edge_links.push(EdgeLink {
            kind: 2,
            navmesh: mesh,
            triangle: tri,
        });
    }
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Grid {
            world: WORLD,
            x: 0,
            y: 0,
        },
        vertices: vec![
            [o, 0.0, 0.0],
            [o + 100.0, 0.0, 0.0],
            [o + 100.0, 100.0, 0.0],
            [o, 100.0, 0.0],
        ],
        triangles: vec![t0, t1],
        edge_links,
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [100.0, 100.0],
            min: [o, 0.0, 0.0],
            max: [o + 100.0, 100.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn xclc(x: i32, y: i32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(&y.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

/// The one exterior cell of a one-cell worldspace, its temporary children
/// being `temporary`. The master writes the worldspace and cell records, a
/// patch writes only the children.
fn exterior(p: &mut Plugin, with_records: bool, temporary: Vec<Record>) {
    let mut world_children = Group::new(GroupKind::WorldChildren(WORLD));
    let mut block = Group::new(GroupKind::ExteriorBlock { y: 0, x: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { y: 0, x: 0 });
    if with_records {
        let cell = p.push_record(
            Record::new(tag::CELL, EXT)
                .zsub(tag::EDID, "Field")
                .sub(b"DATA", [0u8, 0])
                .sub(b"XCLC", xclc(0, 0)),
        );
        sub.entries.push(Entry::Record(cell));
    }
    let mut temp = Group::new(GroupKind::CellTemporary(EXT));
    for r in temporary {
        temp.entries.push(Entry::Record(p.push_record(r)));
    }
    let mut children = Group::new(GroupKind::CellChildren(EXT));
    children.entries.push(Entry::Group(temp));
    sub.entries.push(Entry::Group(children));
    block.entries.push(Entry::Group(sub));
    world_children.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    if with_records {
        let world = p.push_record(
            Record::new(tag::WRLD, WORLD)
                .zsub(tag::EDID, "SmallWorld")
                .sub(b"DATA", [0u8]),
        );
        top.entries.push(Entry::Record(world));
    }
    top.entries.push(Entry::Group(world_children));
    p.groups.push(top);
}

fn navm(form_id: u32, geometry: &Geometry) -> Record {
    Record::new(tag::NAVM, form_id).sub(b"NVNM", geometry.to_bytes())
}

fn master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    // Mesh 1 at x 0..100 links its right side to mesh 2's triangle 1, and
    // mesh 2's triangle 1 links its left side back.
    let mut back = square(100.0, None);
    back.triangles[1].edges[2] = 0;
    back.triangles[1].flags |= vanilla::flags::EDGE_2_0_LINK;
    back.edge_links.push(EdgeLink {
        kind: 2,
        navmesh: NAVM_1,
        triangle: 0,
    });
    exterior(
        &mut p,
        true,
        vec![
            navm(NAVM_1, &square(0.0, Some((NAVM_2, 1)))),
            navm(NAVM_2, &back),
        ],
    );
    p
}

fn patch_deleting_navm_2() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into()],
        ..Default::default()
    });
    exterior(
        &mut p,
        false,
        vec![Record::new(tag::NAVM, NAVM_2).with_flags(flags::DELETED)],
    );
    p
}

fn order_of(files: &[(&str, Plugin)]) -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, plugin) in files {
        let bytes = plugin.to_bytes();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    order
}

#[test]
fn the_master_alone_links_both_ways() {
    let order = order_of(&[("Master.esm", master())]);
    let (graph, stats) = vanilla::from_load_order(&order, |_| true);
    assert_eq!(stats.meshes, 2);
    assert_eq!(stats.edge_links, 2);
    assert_eq!(stats.resolved_links, 2);
    assert_eq!(stats.repaired_links, 0);
    assert_eq!(stats.deleted_meshes, 0);
    assert!(graph
        .find_path(NodeId::new(0, 1), NodeId::new(1, 0))
        .is_some());
}

#[test]
fn a_deleted_navmesh_drops_out_and_its_neighbour_link_is_repaired() {
    let order = order_of(&[
        ("Master.esm", master()),
        ("Dirty.esp", patch_deleting_navm_2()),
    ]);
    let (graph, stats) = vanilla::from_load_order(&order, |_| true);
    assert_eq!(stats.meshes, 1);
    assert_eq!(stats.deleted_meshes, 1);
    assert_eq!(stats.edge_links, 1);
    assert_eq!(stats.repaired_links, 1);
    assert_eq!(stats.resolved_links, 0);
    assert_eq!(stats.unresolved_links, 0);
    assert_eq!(graph.meshes().len(), 1);
    assert_eq!(graph.triangle(NodeId::new(0, 0)).neighbors[1], None);

    // The cell index agrees: only the surviving navmesh is filed.
    let index = order.build_cell_index();
    let cell = index.cell_by_editor_id("Field").unwrap();
    assert_eq!(index.navmeshes(cell).len(), 1);
    let report = order.dirty_report();
    assert_eq!(report.plugin("Dirty.esp").unwrap().deleted_navmeshes, 1);
}
