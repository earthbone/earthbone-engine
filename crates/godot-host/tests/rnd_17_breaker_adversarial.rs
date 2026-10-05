// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 enable-parent chains follow the parent state, including live changes.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(object: u32, initially_disabled: bool, parent: Option<(u32, u32)>) -> Record {
    let mut record = Record::new(tag::REFR, object)
        .sub(b"NAME", 0x800_u32.to_le_bytes())
        .sub(b"DATA", [0; 24]);
    if initially_disabled {
        record.flags |= flags::INITIALLY_DISABLED;
    }
    if let Some((parent, parent_flags)) = parent {
        record = record.sub(
            b"XESP",
            [parent.to_le_bytes(), parent_flags.to_le_bytes()].concat(),
        );
    }
    record
}

#[test]
fn rnd_17_parent_chain_overrides_initial_flags_and_updates_descendants() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800));
    plugin.push_top(reference(0x900, true, None));
    plugin.push_top(reference(0x901, true, Some((0x900, 1))));
    plugin.push_top(reference(0x902, true, Some((0x901, 0))));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();

    assert!(World::authored_reference_enabled(&order, id(0x901)));
    assert!(World::authored_reference_enabled(&order, id(0x902)));

    let mut world = World::new(order);
    for enabled in [false, true, false, true] {
        world.set_reference_enabled(id(0x900), enabled);
        assert_eq!(world.reference_enabled(id(0x901)), !enabled);
        assert_eq!(world.reference_enabled(id(0x902)), !enabled);
    }
}
