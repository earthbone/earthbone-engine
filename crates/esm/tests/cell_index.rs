// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The cell index over a small synthetic order: one master with an interior
//! cell and a two-cell worldspace, one patch that moves a reference into
//! the neighbouring exterior cell and deletes another. Both files round
//! trip through bytes so every placement comes from the parser.

use soe_esm::records::refr::Refr;
use soe_esm::{
    plugin::flags, tag, Entry, FileHeader, GlobalFormId, GridCell, Group, GroupKind, LoadOrder,
    Plugin, Record,
};

const WORLD: u32 = 0x100;
const INTERIOR: u32 = 0x200;
const WORLD_CELL: u32 = 0x201;
const EXT_A: u32 = 0x202;
const EXT_B: u32 = 0x203;
const STATIC: u32 = 0x300;
const REF_INT_TEMP: u32 = 0x400;
const REF_INT_TEMP_2: u32 = 0x401;
const REF_INT_PERSIST: u32 = 0x402;
const REF_EXT_A: u32 = 0x403;
const REF_WORLD_PERSIST: u32 = 0x404;
const ACTOR_INT: u32 = 0x405;
const LAND_A: u32 = 0x500;
const NAVM_A: u32 = 0x501;
const NAVM_A_2: u32 = 0x502;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn refr(form_id: u32) -> Record {
    Record::new(tag::REFR, form_id).sub(b"NAME", STATIC.to_le_bytes())
}

fn xclc(x: i32, y: i32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(&y.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(tag::STAT, STATIC).zsub(tag::EDID, "Rock"));

    let interior = p.push_record(
        Record::new(tag::CELL, INTERIOR)
            .zsub(tag::EDID, "Shared")
            .sub(b"DATA", [1u8, 0]),
    );
    let mut persistent = Group::new(GroupKind::CellPersistent(INTERIOR));
    persistent
        .entries
        .push(Entry::Record(p.push_record(refr(REF_INT_PERSIST))));
    let mut temporary = Group::new(GroupKind::CellTemporary(INTERIOR));
    temporary
        .entries
        .push(Entry::Record(p.push_record(refr(REF_INT_TEMP))));
    temporary
        .entries
        .push(Entry::Record(p.push_record(refr(REF_INT_TEMP_2))));
    temporary.entries.push(Entry::Record(p.push_record(
        Record::new(tag::ACHR, ACTOR_INT).sub(b"NAME", STATIC.to_le_bytes()),
    )));
    let mut children = Group::new(GroupKind::CellChildren(INTERIOR));
    children.entries.push(Entry::Group(persistent));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(interior));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut cell_top = Group::new(GroupKind::Top(tag::CELL));
    cell_top.entries.push(Entry::Group(block));
    p.groups.push(cell_top);

    let world = p.push_record(
        Record::new(tag::WRLD, WORLD)
            .zsub(tag::EDID, "SmallWorld")
            .sub(b"DATA", [0u8]),
    );
    let mut world_children = Group::new(GroupKind::WorldChildren(WORLD));
    let world_cell = p.push_record(
        Record::new(tag::CELL, WORLD_CELL)
            .sub(b"DATA", [0u8, 0])
            .sub(b"XCLC", xclc(0, 0)),
    );
    world_children.entries.push(Entry::Record(world_cell));
    let mut world_persistent = Group::new(GroupKind::CellPersistent(WORLD_CELL));
    world_persistent
        .entries
        .push(Entry::Record(p.push_record(refr(REF_WORLD_PERSIST))));
    let mut world_cell_children = Group::new(GroupKind::CellChildren(WORLD_CELL));
    world_cell_children
        .entries
        .push(Entry::Group(world_persistent));
    world_children
        .entries
        .push(Entry::Group(world_cell_children));

    let mut block = Group::new(GroupKind::ExteriorBlock { y: 0, x: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { y: 0, x: 0 });
    for (cell_id, x, edid, refs) in [
        (EXT_A, 0, "Shared", vec![REF_EXT_A]),
        (EXT_B, 1, "EastOfShared", vec![]),
    ] {
        let cell = p.push_record(
            Record::new(tag::CELL, cell_id)
                .zsub(tag::EDID, edid)
                .sub(b"DATA", [0u8, 0])
                .sub(b"XCLC", xclc(x, 0)),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(cell_id));
        for r in refs {
            temporary
                .entries
                .push(Entry::Record(p.push_record(refr(r))));
        }
        if cell_id == EXT_A {
            temporary
                .entries
                .push(Entry::Record(p.push_record(Record::new(tag::LAND, LAND_A))));
            temporary.entries.push(Entry::Record(
                p.push_record(Record::new(tag::NAVM, NAVM_A_2)),
            ));
            temporary
                .entries
                .push(Entry::Record(p.push_record(Record::new(tag::NAVM, NAVM_A))));
        }
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        children.entries.push(Entry::Group(temporary));
        sub.entries.push(Entry::Record(cell));
        sub.entries.push(Entry::Group(children));
    }
    block.entries.push(Entry::Group(sub));
    world_children.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries.push(Entry::Record(world));
    top.entries.push(Entry::Group(world_children));
    p.groups.push(top);
    p
}

/// Moves `REF_INT_TEMP` from the interior into the eastern exterior cell,
/// deletes `REF_INT_TEMP_2`, and re-places `REF_EXT_A` in the same cell it
/// came from so an override that does not move a reference stays put.
fn patch() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into()],
        ..Default::default()
    });
    let world_cell = p.push_record(
        Record::new(tag::CELL, WORLD_CELL)
            .sub(b"DATA", [0u8, 0])
            .sub(b"XCLC", xclc(0, 0)),
    );
    let mut world_children = Group::new(GroupKind::WorldChildren(WORLD));
    world_children.entries.push(Entry::Record(world_cell));
    let mut block = Group::new(GroupKind::ExteriorBlock { y: 0, x: 0 });
    let mut sub = Group::new(GroupKind::ExteriorSubBlock { y: 0, x: 0 });
    for (cell_id, x, refs) in [(EXT_A, 0, vec![REF_EXT_A]), (EXT_B, 1, vec![REF_INT_TEMP])] {
        let cell = p.push_record(
            Record::new(tag::CELL, cell_id)
                .sub(b"DATA", [0u8, 0])
                .sub(b"XCLC", xclc(x, 0)),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(cell_id));
        for r in refs {
            temporary
                .entries
                .push(Entry::Record(p.push_record(refr(r))));
        }
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        children.entries.push(Entry::Group(temporary));
        sub.entries.push(Entry::Record(cell));
        sub.entries.push(Entry::Group(children));
    }
    block.entries.push(Entry::Group(sub));
    world_children.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries.push(Entry::Group(world_children));
    p.groups.push(top);

    let interior = p.push_record(
        Record::new(tag::CELL, INTERIOR)
            .zsub(tag::EDID, "Shared")
            .sub(b"DATA", [1u8, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(INTERIOR));
    temporary.entries.push(Entry::Record(
        p.push_record(refr(REF_INT_TEMP_2).with_flags(flags::DELETED)),
    ));
    let mut children = Group::new(GroupKind::CellChildren(INTERIOR));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(interior));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut cell_top = Group::new(GroupKind::Top(tag::CELL));
    cell_top.entries.push(Entry::Group(block));
    p.groups.push(cell_top);
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
fn the_master_alone_files_every_child_under_its_cell() {
    let order = order_of(&[("Master.esm", master())]);
    assert!(order.cell_index().is_none());
    let index = order.build_cell_index();

    assert_eq!(
        index.references(id(INTERIOR)),
        [
            id(REF_INT_PERSIST),
            id(REF_INT_TEMP),
            id(REF_INT_TEMP_2),
            id(ACTOR_INT)
        ]
    );
    assert_eq!(
        index.persistent_references(id(INTERIOR)),
        [id(REF_INT_PERSIST)]
    );
    assert_eq!(
        index.temporary_references(id(INTERIOR)),
        [id(REF_INT_TEMP), id(REF_INT_TEMP_2), id(ACTOR_INT)]
    );
    assert!(index.is_interior(id(INTERIOR)));
    assert!(!index.is_interior(id(EXT_A)));

    assert_eq!(index.references(id(EXT_A)), [id(REF_EXT_A)]);
    assert!(index.references(id(EXT_B)).is_empty());
    assert_eq!(index.land(id(EXT_A)), Some(id(LAND_A)));
    assert_eq!(index.land(id(EXT_B)), None);
    assert_eq!(index.navmeshes(id(EXT_A)), [id(NAVM_A), id(NAVM_A_2)]);

    assert_eq!(index.exterior_cell(id(WORLD), 0, 0), Some(id(EXT_A)));
    assert_eq!(index.exterior_cell(id(WORLD), 1, 0), Some(id(EXT_B)));
    assert_eq!(index.exterior_cell(id(WORLD), 2, 0), None);
    assert_eq!(
        index.grid_of(id(EXT_B)),
        Some(GridCell {
            world: id(WORLD),
            x: 1,
            y: 0
        })
    );
    assert_eq!(index.grid_of(id(INTERIOR)), None);
    assert_eq!(index.grid_of(id(WORLD_CELL)), None);
    assert_eq!(index.world_persistent_cell(id(WORLD)), Some(id(WORLD_CELL)));
    assert_eq!(index.persistent_cell_world(id(WORLD_CELL)), Some(id(WORLD)));
    assert_eq!(index.persistent_cell_world(id(INTERIOR)), None);
    assert_eq!(index.persistent_cell_world(id(EXT_B)), None);
    assert_eq!(index.references(id(WORLD_CELL)), [id(REF_WORLD_PERSIST)]);

    // The interior and one exterior cell share an editor id, the interior wins.
    assert_eq!(index.cell_by_editor_id("Shared"), Some(id(INTERIOR)));
    assert_eq!(index.cell_by_editor_id("EastOfShared"), Some(id(EXT_B)));
    assert_eq!(index.cell_by_editor_id("Nowhere"), None);

    assert_eq!(index.cell_count(), 4);
    assert_eq!(index.placed_count(), 6);
    assert_eq!(index.grid_count(), 2);
    assert_eq!(index.world_count(), 1);

    let mut counted: Vec<_> = index.cells().collect();
    counted.sort_unstable();
    assert_eq!(
        counted,
        [
            (id(INTERIOR), 4),
            (id(WORLD_CELL), 1),
            (id(EXT_A), 1),
            (id(EXT_B), 0)
        ]
    );
}

#[test]
fn a_patch_that_moves_a_reference_wins_and_a_deleted_one_disappears() {
    let order = order_of(&[("Master.esm", master()), ("Patch.esp", patch())]);
    order.build_cell_index();
    let index = order.cell_index().unwrap();

    assert_eq!(
        index.references(id(INTERIOR)),
        [id(REF_INT_PERSIST), id(ACTOR_INT)]
    );
    assert_eq!(index.references(id(EXT_B)), [id(REF_INT_TEMP)]);
    assert_eq!(index.references(id(EXT_A)), [id(REF_EXT_A)]);
    assert_eq!(index.placed_count(), 5);

    // The winner is the patch's record, and its own placement agrees.
    let moved = order.winner_ref(id(REF_INT_TEMP)).unwrap();
    assert_eq!(moved.file, 1);
    let view = Refr::from_record(order.record(moved)).unwrap();
    assert_eq!(order.resolve(1, view.cell.unwrap()), id(EXT_B));
    assert_eq!(index.grid_of(id(EXT_B)).unwrap().x, 1);
}

#[test]
fn adding_a_file_drops_the_index() {
    let mut order = order_of(&[("Master.esm", master())]);
    order.build_cell_index();
    assert!(order.cell_index().is_some());
    let bytes = patch().to_bytes();
    order
        .add("Patch.esp", Plugin::parse(&bytes).unwrap())
        .unwrap();
    assert!(order.cell_index().is_none());
    assert_eq!(order.build_cell_index().placed_count(), 5);
}

#[test]
fn concurrent_readers_share_one_cached_cell_index() {
    let order = std::sync::Arc::new(order_of(&[("Master.esm", master())]));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let order = order.clone();
            std::thread::spawn(move || {
                let index = order.build_cell_index();
                assert_eq!(index.references(id(INTERIOR)).len(), 4);
                std::ptr::from_ref(index) as usize
            })
        })
        .collect();
    let pointers: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert!(pointers.iter().all(|p| *p == pointers[0]));
}

#[test]
fn tst_03_cell_index_counts_deletions_and_prefers_the_lowest_tied_cell_id() {
    let order = order_of(&[("Master.esm", master()), ("Patch.esp", patch())]);
    let index = order.build_cell_index();
    assert_eq!(index.deleted_count(), 1);
    assert_eq!(index.world_count(), 1);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag::CELL, 0x600)
            .zsub(tag::EDID, "InteriorBeatsExterior")
            .sub(b"DATA", [0u8, 0]),
    );
    plugin.push_top(
        Record::new(tag::CELL, 0x601)
            .zsub(tag::EDID, "InteriorBeatsExterior")
            .sub(b"DATA", [1u8, 0]),
    );
    for form_id in [0x700, 0x701] {
        plugin.push_top(
            Record::new(tag::CELL, form_id)
                .zsub(tag::EDID, "DuplicateInterior")
                .sub(b"DATA", [1u8, 0]),
        );
    }
    let tied = order_of(&[("Tied.esm", plugin)]);
    assert_eq!(
        tied.build_cell_index()
            .cell_by_editor_id("DuplicateInterior"),
        Some(id(0x700))
    );
    assert_eq!(
        tied.build_cell_index()
            .cell_by_editor_id("InteriorBeatsExterior"),
        Some(id(0x601))
    );
}
