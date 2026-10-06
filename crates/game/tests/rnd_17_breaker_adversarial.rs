// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-17 breaker: XESP owns child enable state and deletion remains final.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn rnd_17_parent_state_overrides_child_initial_flag_and_cannot_revive_deleted_child() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag::REFR, 0x900)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    plugin.push_top(
        Record::new(tag::REFR, 0x901)
            .with_flags(flags::INITIALLY_DISABLED)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(
                b"XESP",
                [0x900_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(
        Record::new(tag::REFR, 0x902)
            .with_flags(flags::DELETED)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(
                b"XESP",
                [0x900_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut world = World::new(order);

    assert!(world.reference_enabled(id(0x901)));
    assert!(world.ref_placement(id(0x901)).unwrap().visible);
    world.set_reference_enabled(id(0x900), false);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(!world.ref_placement(id(0x901)).unwrap().visible);
    world.set_reference_enabled(id(0x900), true);
    assert!(world.reference_enabled(id(0x901)));
    assert!(!world.reference_enabled(id(0x902)));
    assert!(!world.ref_placement(id(0x902)).unwrap().visible);
}
