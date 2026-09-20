// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A dirty patch over a small master, the way the shipped DLC masters are
//! dirty: one identical-to-master override, one deleted reference that a
//! third plugin still links to, one deleted navmesh. The engine keeps the
//! deleted reference resolvable, files it apart from what a loader draws,
//! drops the navmesh, and counts all three per plugin. Every file round
//! trips through bytes so the flags come from the parser.

use soe_esm::records::refr::Refr;
use soe_esm::{
    plugin::flags, tag, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin,
    Record,
};

const INTERIOR: u32 = 0x200;
const ROCK: u32 = 0x300;
const REF_A: u32 = 0x400;
const REF_B: u32 = 0x401;
const NAVM_1: u32 = 0x501;
const NAVM_2: u32 = 0x502;
const LINK_REF: u32 = 0x800;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn refr(form_id: u32) -> Record {
    Record::new(tag::REFR, form_id).sub(b"NAME", ROCK.to_le_bytes())
}

/// One interior cell with its temporary children, as the only group tree of
/// `p`. The cell record itself is written by the master and by nobody else.
fn cell_group(p: &mut Plugin, cell: Option<Record>, temporary: Vec<Record>) {
    let mut temp = Group::new(GroupKind::CellTemporary(INTERIOR));
    for r in temporary {
        temp.entries.push(Entry::Record(p.push_record(r)));
    }
    let mut children = Group::new(GroupKind::CellChildren(INTERIOR));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    if let Some(cell) = cell {
        sub.entries.push(Entry::Record(p.push_record(cell)));
    }
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(tag::CELL));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
}

fn master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(tag::STAT, ROCK).zsub(tag::EDID, "Rock"));
    let cell = Record::new(tag::CELL, INTERIOR)
        .zsub(tag::EDID, "Room")
        .sub(b"DATA", [1u8, 0]);
    cell_group(
        &mut p,
        Some(cell),
        vec![
            refr(REF_A),
            refr(REF_B),
            Record::new(tag::NAVM, NAVM_1),
            Record::new(tag::NAVM, NAVM_2),
        ],
    );
    p
}

/// Repeats the rock byte for byte, deletes `REF_B` and `NAVM_1`.
fn dirty() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into()],
        ..Default::default()
    });
    p.push_top(Record::new(tag::STAT, ROCK).zsub(tag::EDID, "Rock"));
    cell_group(
        &mut p,
        None,
        vec![
            refr(REF_B).with_flags(flags::DELETED),
            Record::new(tag::NAVM, NAVM_1).with_flags(flags::DELETED),
        ],
    );
    p
}

/// Links a new reference to the deleted one and repeats `REF_A` from the
/// master, which is an identical override even though `Dirty.esp` sits
/// between the two files.
fn linker() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into(), "Dirty.esp".into()],
        ..Default::default()
    });
    let mut xlkr = Vec::new();
    xlkr.extend_from_slice(&0u32.to_le_bytes());
    xlkr.extend_from_slice(&REF_B.to_le_bytes());
    cell_group(
        &mut p,
        None,
        vec![
            refr(REF_A),
            Record::new(tag::REFR, 0x0200_0000 | LINK_REF)
                .sub(b"NAME", ROCK.to_le_bytes())
                .sub(b"XLKR", xlkr),
        ],
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
fn a_deleted_reference_stays_resolvable_and_out_of_the_drawn_list() {
    let order = order_of(&[
        ("Master.esm", master()),
        ("Dirty.esp", dirty()),
        ("Linker.esp", linker()),
    ]);
    let index = order.build_cell_index();

    assert_eq!(
        index.references(id(0, INTERIOR)),
        [id(0, REF_A), id(2, LINK_REF)]
    );
    assert_eq!(index.deleted_references(id(0, INTERIOR)), [id(0, REF_B)]);
    assert_eq!(index.placed_count(), 2);
    assert_eq!(index.deleted_count(), 1);
    assert_eq!(index.navmeshes(id(0, INTERIOR)), [id(0, NAVM_2)]);
    assert!(index.deleted_references(id(0, NAVM_2)).is_empty());

    let winner = order
        .winner(id(0, REF_B))
        .expect("the deleted winner is in the table");
    assert!(winner.is_deleted());
    assert_eq!(order.winner_ref(id(0, REF_B)).unwrap().file, 1);

    let link = Refr::from_record(order.winner(id(2, LINK_REF)).unwrap()).unwrap();
    let target = order.resolve(2, link.linked_refs[0].reference);
    assert_eq!(target, id(0, REF_B));
    assert!(order.contains(target));
}

#[test]
fn the_dirty_report_counts_per_plugin() {
    let order = order_of(&[
        ("Master.esm", master()),
        ("Dirty.esp", dirty()),
        ("Linker.esp", linker()),
    ]);
    let report = order.dirty_report();
    assert_eq!(report.plugins.len(), 3);

    let master = report.plugin("master.esm").unwrap();
    assert!(master.is_clean(), "{master:?}");

    let dirty = report.plugin("Dirty.esp").unwrap();
    assert_eq!(dirty.identical_to_master, 1);
    assert_eq!(dirty.deleted_references, 1);
    assert_eq!(dirty.deleted_navmeshes, 1);
    assert_eq!(dirty.deleted_other, 0);

    let linker = report.plugin("Linker.esp").unwrap();
    assert_eq!(linker.identical_to_master, 1, "{linker:?}");
    assert_eq!(linker.deleted_references, 0);
    assert!(!linker.is_clean());

    assert_eq!(report.identical_to_master(), 2);
    assert_eq!(report.deleted_references(), 1);
    assert_eq!(report.deleted_navmeshes(), 1);
    assert!(report.plugin("Nowhere.esp").is_none());
}

#[test]
fn a_changed_override_is_not_identical() {
    let mut p = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into()],
        ..Default::default()
    });
    p.push_top(Record::new(tag::STAT, ROCK).zsub(tag::EDID, "Boulder"));
    let order = order_of(&[("Master.esm", master()), ("Changed.esp", p)]);
    let report = order.dirty_report();
    assert!(report.plugin("Changed.esp").unwrap().is_clean());
}
