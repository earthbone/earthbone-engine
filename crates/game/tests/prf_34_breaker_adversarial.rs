// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::{world::ReferenceChanges, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(parent_flags: u32) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800));
    plugin.push_top(
        Record::new(tag::REFR, 0x900)
            .with_flags(flags::INITIALLY_DISABLED)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    plugin.push_top(
        Record::new(tag::REFR, 0x901)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(
                b"XESP",
                [0x900_u32.to_le_bytes(), parent_flags.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn xesp_reserved_and_pop_in_bits_do_not_change_enable_inversion() {
    // UESP REFR.wikitext and xEdit wbXESP define bit 0 as inversion, bit 1 as Pop In.
    let world = world(0xffff_fffe);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(!World::authored_reference_enabled(world.order(), id(0x901)));
}

#[test]
fn parent_change_invalidates_a_watched_child_and_repeat_is_quiet() {
    let mut world = world(0);
    let mut observer = ReferenceChanges::default();
    observer.watch(&world, id(0x901));
    assert_eq!(observer.changed(&world), vec![id(0x901)]);
    assert!(observer.changed(&world).is_empty());
    world.set_reference_enabled(id(0x900), true);
    assert!(world.reference_enabled(id(0x901)));
    assert_eq!(observer.changed(&world), vec![id(0x901)]);
    assert!(observer.changed(&world).is_empty());
}

#[test]
fn missing_parent_stays_hidden_after_cache_and_parent_mutation() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(tag::REFR, 0x901)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(
                b"XESP",
                [0x999_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let world = World::new(order);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(!world.reference_enabled(id(0x901)));
}
