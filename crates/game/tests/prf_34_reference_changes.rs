// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-34 cached enable chains and independent observers, from UESP REFR XESP.

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_game::{world::ReferenceChanges, World};
use soe_trace::{Operation, WorkFrame};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag::STAT, 0x800));
    plugin.push_top(Record::new(soe_esm::Tag::new(b"DOOR"), 0x801));
    for (object, disabled, parent) in [
        (0x900, true, None),
        (0x901, false, Some((0x900_u32, 0_u32))),
        (0x902, true, Some((0x901, 1))),
        (0x903, false, None),
    ] {
        let mut record = Record::new(tag::REFR, object)
            .with_flags(if disabled {
                flags::INITIALLY_DISABLED
            } else {
                0
            })
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]);
        if let Some((parent, flags)) = parent {
            record = record.sub(
                b"XESP",
                [parent.to_le_bytes(), flags.to_le_bytes()].concat(),
            );
        }
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

fn attach(world: &World) -> ReferenceChanges {
    let mut changes = ReferenceChanges::default();
    for object in 0x900..=0x903 {
        changes.watch(world, id(object));
    }
    assert_eq!(changes.changed(world).len(), 4);
    changes
}

#[test]
fn prf_34_initial_flags_parent_chains_opposites_and_independent_consumers() {
    let mut world = world();
    let mut render = attach(&world);
    let mut collision = attach(&world);
    assert!(!world.reference_enabled(id(0x901)));
    assert!(world.reference_enabled(id(0x902)));
    world.set_reference_enabled(id(0x900), true);
    let expected = vec![id(0x900), id(0x901), id(0x902)];
    assert_eq!(render.changed(&world), expected);
    assert_eq!(collision.changed(&world), expected);
    assert!(world.reference_enabled(id(0x901)));
    assert!(!world.reference_enabled(id(0x902)));
    world.set_reference_enabled(id(0x900), false);
    assert_eq!(collision.changed(&world), expected);
    assert_eq!(render.changed(&world), expected);
}

#[test]
fn prf_34_idle_sync_and_cached_enable_queries_read_no_records() {
    let world = world();
    let mut changes = attach(&world);
    let frame = WorkFrame::begin();
    for _ in 0..1000 {
        assert!(changes.changed(&world).is_empty());
        assert!(world.reference_enabled(id(0x902)));
    }
    assert_eq!(frame.counts().get(Operation::RecordLookups), 0);
}

#[test]
fn prf_34_direct_mutation_placement_deletion_and_unchanged_access() {
    let mut world = world();
    let mut changes = attach(&world);
    world.reference(id(0x903)).unwrap().position = [1.0, 2.0, 3.0];
    assert_eq!(changes.changed(&world), vec![id(0x903)]);
    world.reference(id(0x903)).unwrap();
    assert!(changes.changed(&world).is_empty());
    world.reference(id(0x900)).unwrap().deleted = true;
    assert_eq!(changes.changed(&world), vec![id(0x902)]);
    assert!(!world.reference_enabled(id(0x902)));
}

#[test]
fn prf_34_cell_reattach_and_save_reload_rebuild_transient_caches() {
    let mut world = world();
    let saved = world.save_state();
    let mut changes = attach(&world);
    world.set_reference_enabled(id(0x900), true);
    assert_eq!(changes.changed(&world).len(), 3);
    world.load_state(&saved).unwrap();
    assert_eq!(changes.changed(&world).len(), 3);
    assert!(!world.reference_enabled(id(0x901)));
    let mut reattached = attach(&world);
    assert!(reattached.changed(&world).is_empty());
    assert_eq!(world.save_state(), saved);
}

#[test]
fn prf_34_installed_enable_cache_matches_winning_vanilla_records() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let ids: Vec<_> = order.winners_of(tag::REFR).map(|(id, _)| id).collect();
    let world = World::from_shared_order(order);
    let mut parents = 0;
    for id in &ids {
        parents += usize::from(
            world
                .record(*id)
                .unwrap()
                .find(soe_esm::Tag::new(b"XESP"))
                .is_some(),
        );
        assert_eq!(
            world.reference_enabled(*id),
            World::authored_reference_enabled(world.order(), *id),
            "{id}"
        );
    }
    assert!(parents > 1000);
    let frame = WorkFrame::begin();
    for id in &ids {
        std::hint::black_box(world.reference_enabled(*id));
    }
    assert_eq!(frame.counts().get(Operation::RecordLookups), 0);
    eprintln!(
        "PRF-34 vanilla references={} parent_links={parents}",
        ids.len()
    );
}

#[test]
fn prf_34_missing_cyclic_and_truncated_parents_remain_hidden_when_cached() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, parent) in [(0x900, 0x901_u32), (0x901, 0x900), (0x902, 0x999)] {
        plugin.push_top(
            Record::new(tag::REFR, object)
                .sub(b"NAME", 0x800_u32.to_le_bytes())
                .sub(b"DATA", [0; 24])
                .sub(
                    b"XESP",
                    [parent.to_le_bytes(), 1_u32.to_le_bytes()].concat(),
                ),
        );
    }
    plugin.push_top(
        Record::new(tag::REFR, 0x903)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(b"XESP", [0; 3]),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut world = World::new(order);
    let mut changes = attach(&world);
    world.set_reference_enabled(id(0x900), true);
    assert!(changes.changed(&world).is_empty());
    for object in 0x900..=0x903 {
        assert!(!world.reference_enabled(id(object)));
        assert!(!world.reference_enabled(id(object)));
    }
}

#[test]
fn prf_34_winning_parent_dependency_uses_the_override_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(
        Record::new(tag::REFR, 0x900)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    let mut update = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    update.push_top(
        Record::new(tag::REFR, 0x0100_0900)
            .with_flags(flags::INITIALLY_DISABLED)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    update.push_top(
        Record::new(tag::REFR, 0x901)
            .sub(b"NAME", 0x800_u32.to_le_bytes())
            .sub(b"DATA", [0; 24])
            .sub(
                b"XESP",
                [0x0100_0900_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Update.esm", update).unwrap();
    let mut world = World::new(order);
    let mut changes = ReferenceChanges::default();
    changes.watch(&world, id(0x901));
    assert_eq!(changes.changed(&world), vec![id(0x901)]);
    assert!(!world.reference_enabled(id(0x901)));
    world.set_reference_enabled(
        GlobalFormId {
            file: 1,
            object: 0x900,
        },
        true,
    );
    assert_eq!(changes.changed(&world), vec![id(0x901)]);
    assert!(world.reference_enabled(id(0x901)));
}

#[test]
fn prf_34_door_open_close_and_disable_notify_both_consumers() {
    let mut world = world();
    world.reference(id(0x903)).unwrap().base = Some(id(0x801));
    let mut render = attach(&world);
    let mut collision = attach(&world);
    for _ in 0..10 {
        assert!(world.begin_door_opening(id(0x903)));
        assert_eq!(render.changed(&world), vec![id(0x903)]);
        assert_eq!(collision.changed(&world), vec![id(0x903)]);
        assert!(world.reference_enabled(id(0x903)));
        assert!(world.begin_door_closing(id(0x903)));
        assert_eq!(collision.changed(&world), vec![id(0x903)]);
        assert_eq!(render.changed(&world), vec![id(0x903)]);
    }
    world.set_reference_enabled(id(0x903), false);
    assert_eq!(render.changed(&world), vec![id(0x903)]);
    assert_eq!(collision.changed(&world), vec![id(0x903)]);
    assert!(!world.reference_enabled(id(0x903)));
}

#[test]
fn prf_34_unwatched_references_stay_quiet_and_watching_again_reports_them_once() {
    let mut world = world();
    let mut changes = attach(&world);
    changes.unwatch(id(0x900));
    world.set_reference_enabled(id(0x900), true);
    // The unwatched parent is silent, its watched children still report.
    assert_eq!(changes.changed(&world), vec![id(0x901), id(0x902)]);
    changes.watch(&world, id(0x900));
    changes.watch(&world, id(0x900));
    assert_eq!(changes.changed(&world), vec![id(0x900)]);
    world.set_reference_enabled(id(0x900), false);
    assert_eq!(
        changes.changed(&world),
        vec![id(0x900), id(0x901), id(0x902)]
    );
}

#[test]
fn prf_34_quiet_watch_reports_the_enable_state_and_only_later_changes() {
    let mut world = world();
    let mut changes = ReferenceChanges::default();
    assert_eq!(changes.watch_quiet(&world, id(0x900)), Some(false));
    assert_eq!(changes.watch_quiet(&world, id(0x901)), Some(false));
    assert_eq!(changes.watch_quiet(&world, id(0x903)), Some(true));
    assert_eq!(changes.watch_quiet(&world, id(0x903)), None);
    assert!(changes.changed(&world).is_empty());
    world.set_reference_enabled(id(0x900), true);
    assert_eq!(changes.changed(&world), vec![id(0x900), id(0x901)]);
}

#[test]
fn prf_34_budgeted_changes_leave_the_rest_queued_for_the_next_call() {
    let mut world = world();
    let mut changes = attach(&world);
    world.set_reference_enabled(id(0x900), true);
    let first = changes.changed_within(&world, std::time::Duration::ZERO);
    assert_eq!(first, vec![id(0x900)]);
    let rest = changes.changed_within(&world, std::time::Duration::from_secs(60));
    assert_eq!(rest, vec![id(0x901), id(0x902)]);
    assert!(changes
        .changed_within(&world, std::time::Duration::ZERO)
        .is_empty());
}
