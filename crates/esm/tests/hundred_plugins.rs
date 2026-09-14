// SPDX-License-Identifier: GPL-3.0-or-later

//! A synthetic hundred-plugin load order in the shape a Wabbajack list has:
//! one base master with a worldspace, a handful of light plugins, and many
//! patches that override the same cells and references. Checks that the
//! winner is always the last file to touch a form and that every reference
//! in the worldspace resolves to a base object somewhere in the order.

use soe_esm::records::{cell::Cell, refr::Refr};
use soe_esm::tag;
use soe_esm::{
    plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Slot, Tag,
};

const WORLD: u32 = 0x3C;

fn base_master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "base".into(),
        ..Default::default()
    });
    for i in 0..50u32 {
        p.push_top(Record::new(tag::STAT, 0x1000 + i).zsub(tag::EDID, &format!("Static{i}")));
    }
    let world = p.push_record(
        Record::new(tag::WRLD, WORLD)
            .zsub(tag::EDID, "Tamriel")
            .sub(b"DATA", [0u8]),
    );
    let mut world_children = Group::new(GroupKind::WorldChildren(WORLD));
    let mut next = 0x2000u32;
    for y in -2i16..=2 {
        for x in -2i16..=2 {
            let cell_id = next;
            next += 1;
            let mut xclc = Vec::new();
            xclc.extend_from_slice(&i32::from(x).to_le_bytes());
            xclc.extend_from_slice(&i32::from(y).to_le_bytes());
            xclc.extend_from_slice(&0u32.to_le_bytes());
            let cell = p.push_record(
                Record::new(tag::CELL, cell_id)
                    .zsub(tag::EDID, &format!("Cell_{x}_{y}"))
                    .sub(b"DATA", [0u8, 0])
                    .sub(b"XCLC", xclc),
            );
            let mut temp = Group::new(GroupKind::CellTemporary(cell_id));
            for _ in 0..4 {
                let refr = p.push_record(
                    Record::new(tag::REFR, next).sub(b"NAME", (0x1000 + (next % 50)).to_le_bytes()),
                );
                next += 1;
                temp.entries.push(Entry::Record(refr));
            }
            let mut children = Group::new(GroupKind::CellChildren(cell_id));
            children.entries.push(Entry::Group(temp));
            let mut sub = Group::new(GroupKind::ExteriorSubBlock { y: y / 8, x: x / 8 });
            sub.entries.push(Entry::Record(cell));
            sub.entries.push(Entry::Group(children));
            let mut block = Group::new(GroupKind::ExteriorBlock {
                y: y / 32,
                x: x / 32,
            });
            block.entries.push(Entry::Group(sub));
            world_children.entries.push(Entry::Group(block));
        }
    }
    let mut top = Group::new(GroupKind::Top(tag::WRLD));
    top.entries.push(Entry::Record(world));
    top.entries.push(Entry::Group(world_children));
    p.groups.push(top);
    p
}

#[test]
fn a_hundred_plugin_order_resolves_every_reference_and_picks_the_last_override() {
    let mut order = LoadOrder::new();
    // Round-trip the master through bytes so placements come from the
    // parser, the way they would from a real file.
    let base_bytes = base_master().to_bytes();
    order
        .add("Base.esm", Plugin::parse(&base_bytes).unwrap())
        .unwrap();
    let base = order.file(0);
    let refr_ids: Vec<u32> = base
        .plugin
        .records_of(tag::REFR)
        .map(|r| r.form_id)
        .collect();
    let cell_ids: Vec<u32> = base
        .plugin
        .records_of(tag::CELL)
        .map(|r| r.form_id)
        .collect();
    assert_eq!(cell_ids.len(), 25);
    assert_eq!(refr_ids.len(), 100);

    let mut expected_winner: std::collections::HashMap<GlobalFormId, String> = Default::default();
    for n in 1..=99u32 {
        let light = n % 7 == 0;
        let name = format!("Plugin{n:03}.{}", if light { "esl" } else { "esp" });
        let mut p = Plugin::new(FileHeader {
            flags: if light { flags::LIGHT_MASTER } else { 0 },
            masters: vec!["Base.esm".into()],
            ..Default::default()
        });
        // Override a cell and two of its references, plus one new static
        // and one reference to it placed in the same cell.
        let cell = cell_ids[(n as usize) % cell_ids.len()];
        p.push_top(
            Record::new(tag::CELL, cell)
                .zsub(tag::EDID, &format!("Cell_from_{n}"))
                .sub(b"DATA", [0u8, 0]),
        );
        expected_winner.insert(
            GlobalFormId {
                file: 0,
                object: cell,
            },
            format!("Cell_from_{n}"),
        );
        for k in 0..2 {
            let r = refr_ids[((n as usize) * 3 + k) % refr_ids.len()];
            p.push_top(
                Record::new(tag::REFR, r)
                    .zsub(tag::EDID, &format!("Ref_from_{n}"))
                    .sub(b"NAME", (0x1000 + (r % 50)).to_le_bytes()),
            );
            expected_winner.insert(GlobalFormId { file: 0, object: r }, format!("Ref_from_{n}"));
        }
        let own_static = 0x01000800u32;
        p.push_top(Record::new(tag::STAT, own_static).zsub(tag::EDID, &format!("NewStatic{n}")));
        p.push_top(Record::new(tag::REFR, 0x01000801).sub(b"NAME", own_static.to_le_bytes()));
        let idx = order.add(&name, p).unwrap();
        match order.file(idx).slot {
            Slot::Light(_) => assert!(light),
            Slot::Regular(_) => assert!(!light),
        }
    }
    assert_eq!(order.files().len(), 100);

    for (id, name) in &expected_winner {
        let w = order.winner(*id).expect("overridden form has a winner");
        assert_eq!(w.editor_id().as_deref(), Some(name.as_str()), "{id}");
    }

    // Every reference in every file points at a base object the order knows.
    let mut refs = 0;
    for (file_index, file) in order.files().iter().enumerate() {
        for rec in file.plugin.records_of(tag::REFR) {
            let refr = Refr::from_record(rec).unwrap();
            let base = order.resolve(file_index, refr.base);
            assert!(
                order.contains(base),
                "{} refers to unknown base {base}",
                file.name
            );
            assert_eq!(order.winner(base).unwrap().tag, Tag::new(b"STAT"));
            refs += 1;
        }
    }
    assert_eq!(refs, 100 + 99 * 3);

    // Winning cells still decode, and the base's placement survives in the
    // base record even where a patch overrides the header.
    for id in cell_ids {
        let g = GlobalFormId {
            file: 0,
            object: id,
        };
        let winner = order.winner(g).unwrap();
        let cell = Cell::from_record(winner, false).unwrap();
        assert!(!cell.is_interior());
        let original = order.record(order.overrides(g)[0]);
        assert_eq!(original.placement.world, Some(WORLD));
    }

    // Light plugins pack into FE slots in the order they were added.
    let first_light = order.position_of("Plugin007.esl").unwrap();
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: first_light as u16,
            object: 0x800
        }),
        0xFE000800
    );
    let second_light = order.position_of("Plugin014.esl").unwrap();
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: second_light as u16,
            object: 0x800
        }),
        0xFE001800
    );
    // Regular slots skip the light files: Plugin008.esp follows the master
    // and six regular plugins (Plugin007 is light), so it takes slot 7.
    let p8 = order.position_of("Plugin008.esp").unwrap();
    assert_eq!(
        order.runtime_id(GlobalFormId {
            file: p8 as u16,
            object: 0x800
        }) >> 24,
        7
    );

    // Overridden forms are exactly the ones a patch touched.
    let overridden: std::collections::HashSet<_> = order.overridden().collect();
    assert_eq!(overridden, expected_winner.keys().copied().collect());
}
