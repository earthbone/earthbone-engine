// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for reference enable invalidation.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::{world::ReferenceChanges, World};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn test_world(parent_flags: u32) -> World {
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

fn observe(world: &World) -> ReferenceChanges {
    let mut changes = ReferenceChanges::default();
    changes.watch(world, id(0x901));
    assert_eq!(changes.changed(world), vec![id(0x901)]);
    changes
}

#[test]
fn prf_34_reserved_and_pop_in_bits_do_not_invert_enable_parent() {
    // UESP REFR XESP and xEdit wbXESP define bit 0 as opposite and bit 1 as
    // Pop In. Other bits are unused and must not alter effective enable state.
    let world = test_world(0xffff_fffe);
    assert!(!world.reference_enabled(id(0x901)));
    let mut changes = observe(&world);
    assert!(changes.changed(&world).is_empty());
}

#[test]
fn prf_34_parent_toggles_coalesce_to_the_last_presented_state() {
    let mut world = test_world(0);
    let mut changes = observe(&world);
    world.set_reference_enabled(id(0x900), true);
    world.set_reference_enabled(id(0x900), false);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(changes.changed(&world).is_empty());
    world.set_reference_enabled(id(0x900), true);
    assert_eq!(changes.changed(&world), vec![id(0x901)]);
    assert!(world.ref_placement(id(0x901)).unwrap().visible);
}

#[test]
fn prf_34_each_observer_receives_parent_changes_even_after_another_drains() {
    let mut world = test_world(1);
    let mut render = observe(&world);
    let mut collision = observe(&world);
    world.set_reference_enabled(id(0x900), true);
    assert_eq!(render.changed(&world), vec![id(0x901)]);
    assert_eq!(collision.changed(&world), vec![id(0x901)]);
    assert!(!world.reference_enabled(id(0x901)));
}

