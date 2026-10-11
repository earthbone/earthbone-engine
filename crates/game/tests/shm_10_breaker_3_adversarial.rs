// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::extra_data::{ReferenceExtra, ReferenceExtraData, ReferenceExtraKind};
use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::ObjectRefHandle;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::World;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn live_reference() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 10));
    plugin.push_top(Record::new(Tag(*b"ALCH"), 11));
    plugin.push_top(Record::new(Tag(*b"OTFT"), 12));
    plugin.push_top(Record::new(Tag(*b"REFR"), 1).sub(Tag(*b"NAME"), 10_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("SHM10.esm", plugin).expect("fixture loads");
    let mut world = World::new(order);
    world.reference(id(1)).expect("placed reference loads");
    world
}

#[test]
fn outfit_and_poison_extras_read_mutated_live_stack_and_keep_unsigned_count() {
    let mut world = live_reference();
    let initial = Stack::new(
        1,
        vec![
            Extra {
                kind: 142,
                fields: vec![Field::Form(Some(id(12)))],
            },
            Extra {
                kind: 62,
                fields: vec![Field::Form(Some(id(11))), Field::Word(u32::MAX)],
            },
        ],
    )
    .expect("fixture extras are valid");
    world
        .reference(id(1))
        .expect("reference remains live")
        .inventory
        .restore(id(10), 1, vec![initial])
        .expect("inventory restore succeeds");
    let handle = ObjectRefHandle(world.runtime_id(id(1)));

    assert_eq!(
        world.extra(handle, ReferenceExtraKind::OutfitItem),
        Some(ReferenceExtra::OutfitItem(FormId(world.runtime_id(id(12)))))
    );
    assert_eq!(
        world.extra(handle, ReferenceExtraKind::Poison),
        Some(ReferenceExtra::Poison {
            poison: FormId(world.runtime_id(id(11))),
            count: u32::MAX,
        })
    );

    let changed = Stack::new(
        1,
        vec![Extra {
            kind: 62,
            fields: vec![Field::Form(Some(id(11))), Field::Word(3)],
        }],
    )
    .expect("changed poison extra is valid");
    world
        .reference(id(1))
        .expect("reference remains live")
        .inventory
        .restore(id(10), 1, vec![changed])
        .expect("inventory restore succeeds");
    assert_eq!(
        world.extra(handle, ReferenceExtraKind::Poison),
        Some(ReferenceExtra::Poison {
            poison: FormId(world.runtime_id(id(11))),
            count: 3,
        })
    );
    assert_eq!(world.extra(handle, ReferenceExtraKind::OutfitItem), None);
}

#[test]
fn malformed_or_unresolved_inventory_extra_forms_are_absent() {
    let mut world = live_reference();
    let malformed = Stack::new(
        1,
        vec![
            Extra {
                kind: 142,
                fields: vec![Field::Form(None)],
            },
            Extra {
                kind: 62,
                fields: vec![Field::Form(Some(id(0x7fff))), Field::Word(4)],
            },
        ],
    )
    .expect("stored fields are structurally valid");
    world
        .reference(id(1))
        .expect("placed reference loads")
        .inventory
        .restore(id(10), 1, vec![malformed])
        .expect("inventory restore succeeds");
    let handle = ObjectRefHandle(world.runtime_id(id(1)));

    assert_eq!(world.extra(handle, ReferenceExtraKind::OutfitItem), None);
    assert_eq!(world.extra(handle, ReferenceExtraKind::Poison), None);
}
