// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CK AddItem reference moves, original identity, events and SOEW persistence.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    inventory::{Extra, Field, Stack},
    Handle, Pending, World,
};
use soe_papyrus::{ObjectId, Value};

const ITEM: u32 = 0x801;
const LOOSE: u32 = 0x902;
const FIRST: u32 = 0x900;
const SECOND: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(count: i32) -> LoadOrder {
    let mut p = Plugin::new(FileHeader::default());
    for (tag, object) in [(*b"CONT", 0x800), (*b"WEAP", ITEM), (*b"NPC_", 0x810)] {
        p.push_top(Record::new(Tag(tag), object));
    }
    for object in [FIRST, SECOND] {
        p.push_top(Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    }
    p.push_top(
        Record::new(Tag(*b"REFR"), LOOSE)
            .sub(Tag(*b"NAME"), ITEM.to_le_bytes())
            .sub(Tag(*b"XCNT"), count.to_le_bytes())
            .sub(Tag(*b"XOWN"), 0x810_u32.to_le_bytes())
            .sub(Tag(*b"XRNK"), 2_i32.to_le_bytes())
            .sub(Tag(*b"XCHG"), 0x7fc0_0042_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Items.esm", p).unwrap();
    order
}

fn world(count: i32) -> World {
    World::new(order(count))
}

fn allocator() -> impl FnMut(&str, &str) -> ObjectId {
    let mut next = 0;
    move |_, _| {
        next += 1;
        ObjectId(next)
    }
}

fn extras() -> Vec<Extra> {
    vec![
        Extra {
            kind: 37,
            fields: vec![Field::FloatBits(1.5_f32.to_bits())],
        },
        Extra {
            kind: 155,
            fields: vec![Field::Form(Some(id(0x820))), Field::Short(321)],
        },
        Extra {
            kind: 62,
            fields: vec![Field::Form(Some(id(0x821))), Field::Word(1)],
        },
        Extra {
            kind: 153,
            fields: vec![Field::Text(b"Named blade".to_vec())],
        },
    ]
}

fn events(w: &mut World) -> Vec<(ObjectId, String, Vec<Value>)> {
    w.pending
        .drain(..)
        .filter_map(|p| match p {
            Pending::Event { object, name, args } => Some((object, name, args)),
            _ => None,
        })
        .collect()
}

#[test]
fn pickup_moves_loose_quantity_extras_identity_and_world_placement() {
    let mut w = world(3);
    let mut alloc = allocator();
    let state = w.reference(id(LOOSE)).unwrap();
    state.cell = Some(id(0x850));
    let mut metadata = state.item.as_ref().unwrap().extras().to_vec();
    metadata.extend(extras());
    state.item = Some(Stack::new(3, metadata.clone()).unwrap());
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    let state = w.reference_ref(id(LOOSE)).unwrap();
    assert!(state.item.is_none());
    assert!(!state.enabled);
    assert_eq!(state.cell, None);
    let inv = &w.reference_ref(id(FIRST)).unwrap().inventory;
    assert_eq!(inv.count(id(ITEM)), 3);
    let stack = &inv.stacks(id(ITEM))[0];
    assert_eq!(stack.reference(), Some(id(LOOSE)));
    for extra in metadata {
        assert_eq!(stack.extra(extra.kind), Some(&extra));
    }
    let notices = events(&mut w);
    assert_eq!(notices.len(), 2);
    assert_eq!(notices[0].1, "OnItemAdded");
    assert_eq!(notices[0].2[1], Value::Int(3));
    assert_eq!(w.form_arg(&notices[0].2[2]), Some(id(LOOSE)));
    assert_eq!(notices[0].2[3], Value::None);
    assert_eq!(notices[1].1, "OnContainerChanged");
    assert_eq!(w.form_arg(&notices[1].2[0]), Some(id(FIRST)));
    assert_eq!(notices[1].2[1], Value::None);
}

#[test]
fn reference_transfer_selects_only_its_stack_and_notifies_both_holders() {
    let mut w = world(1);
    let mut alloc = allocator();
    w.add_item(id(FIRST), id(ITEM), 5);
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    let original = w
        .reference_ref(id(FIRST))
        .unwrap()
        .inventory
        .stacks(id(ITEM))[0]
        .clone();
    events(&mut w);
    assert!(!w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    assert!(events(&mut w).is_empty());
    assert!(w.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
    assert_eq!(w.item_count(id(FIRST), id(ITEM)), 5);
    assert_eq!(w.item_count(id(SECOND), id(ITEM)), 1);
    assert_eq!(
        w.reference_ref(id(SECOND))
            .unwrap()
            .inventory
            .stacks(id(ITEM)),
        &[original]
    );
    let notices = events(&mut w);
    assert_eq!(
        notices.iter().map(|e| e.1.as_str()).collect::<Vec<_>>(),
        ["OnItemRemoved", "OnItemAdded", "OnContainerChanged"]
    );
    assert_eq!(w.form_arg(&notices[0].2[2]), Some(id(LOOSE)));
    assert_eq!(w.form_arg(&notices[0].2[3]), Some(id(SECOND)));
    assert_eq!(w.form_arg(&notices[1].2[3]), Some(id(FIRST)));
}

#[test]
fn rejected_inputs_and_overflow_leave_the_loose_item_and_events_unchanged() {
    let mut w = world(3);
    let mut alloc = allocator();
    w.reference(id(LOOSE)).unwrap();
    let before = w.reference_ref(id(LOOSE)).unwrap().clone();
    for count in [i32::MIN, -1, 0, 2, i32::MAX] {
        assert!(!w.add_item_notified(id(FIRST), id(LOOSE), count, &mut alloc));
    }
    assert!(!w.add_item_notified(id(0xbad), id(LOOSE), 1, &mut alloc));
    assert!(!w.add_item_notified(id(FIRST), id(SECOND), 1, &mut alloc));
    assert!(!w.add_item_notified(id(LOOSE), id(LOOSE), 1, &mut alloc));
    w.add_item(id(FIRST), id(ITEM), i32::MAX - 2);
    assert!(!w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    assert_eq!(w.reference_ref(id(LOOSE)), Some(&before));
    assert!(events(&mut w).is_empty());
}

#[test]
fn consumed_references_cannot_be_added_again_even_after_save_load() {
    for mode in 0..3 {
        let mut w = world(1);
        let mut alloc = allocator();
        assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
        events(&mut w);
        match mode {
            0 => {
                assert_eq!(
                    w.remove_item_notified(id(FIRST), id(ITEM), 1, None, &mut alloc),
                    1
                );
            }
            1 => w.add_item(id(FIRST), id(ITEM), -1),
            _ => w.reference(id(FIRST)).unwrap().inventory.clear(),
        }
        assert!(events(&mut w).iter().all(|e| e.1 != "OnContainerChanged"));
        let mut restored = world(1);
        restored.load_state(&w.save_state()).unwrap();
        assert!(!restored.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
        assert_eq!(restored.item_count(id(SECOND), id(ITEM)), 0);
    }
}

#[test]
fn partial_removal_keeps_one_reference_identity_on_the_remaining_quantity() {
    let mut w = world(3);
    let mut alloc = allocator();
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    assert_eq!(w.remove_item(id(FIRST), id(ITEM), 1, Some(id(SECOND))), 1);
    assert_eq!(
        w.reference_ref(id(FIRST))
            .unwrap()
            .inventory
            .reference_count(id(LOOSE)),
        2
    );
    assert_eq!(
        w.reference_ref(id(SECOND))
            .unwrap()
            .inventory
            .reference_count(id(LOOSE)),
        0
    );
    assert!(w.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
    assert_eq!(w.item_count(id(FIRST), id(ITEM)), 0);
    assert_eq!(w.item_count(id(SECOND), id(ITEM)), 3);
    assert_eq!(
        w.reference_ref(id(SECOND))
            .unwrap()
            .inventory
            .reference_count(id(LOOSE)),
        2
    );
}

#[test]
fn base_form_removal_transfers_reference_events_and_preserves_identity() {
    let mut w = world(1);
    let mut alloc = allocator();
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    events(&mut w);
    assert_eq!(
        w.remove_item_notified(id(FIRST), id(ITEM), 1, Some(id(SECOND)), &mut alloc),
        1
    );
    let notices = events(&mut w);
    assert_eq!(notices.len(), 3);
    assert_eq!(w.form_arg(&notices[0].2[2]), Some(id(LOOSE)));
    assert_eq!(w.form_arg(&notices[1].2[2]), Some(id(LOOSE)));
    assert_eq!(w.handle_of(notices[2].0), Some(Handle::Form(id(LOOSE))));
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
}

#[test]
fn loose_and_contained_metadata_round_trip_with_stable_bytes_and_hashes() {
    let mut w = world(1);
    let mut alloc = allocator();
    w.reference(id(LOOSE)).unwrap().item = Some(Stack::new(1, extras()).unwrap());
    for collected in [false, true] {
        if collected {
            assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
        }
        let bytes = w.save_state();
        let hash = w.state_hash();
        let mut loaded = world(1);
        loaded.load_state(&bytes).unwrap();
        assert_eq!(loaded.save_state(), bytes);
        assert_eq!(loaded.state_hash(), hash);
        assert!(loaded.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
        assert_eq!(loaded.item_count(id(SECOND), id(ITEM)), 1);
    }
}

#[test]
fn version_fifteen_migrates_authored_loose_items_and_rejects_truncated_new_extension() {
    let mut w = world(3);
    w.reference(id(LOOSE)).unwrap();
    let hash = w.state_hash();
    let mut bytes = w.save_state();
    // CONTEXT: One entry: table count, FormID, stack count and three typed extras.
    let extension = 4 + 6 + 4 + 4 + (1 + 4 + 1 + 1 + 6) + 2 * (1 + 4 + 1 + 4);
    bytes.truncate(bytes.len() - extension - 4);
    let mut loaded = world(3);
    assert!(loaded.load_state(&bytes).is_err());
    bytes[4..6].copy_from_slice(&25_u16.to_le_bytes());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(
        loaded.reference_ref(id(LOOSE)).unwrap().item,
        w.reference_ref(id(LOOSE)).unwrap().item
    );
}

#[test]
fn corrupt_loose_metadata_and_duplicate_reference_identity_reject_atomically() {
    let mut w = world(1);
    let mut alloc = allocator();
    w.reference(id(LOOSE)).unwrap();
    let bytes = w.save_state();
    let hash = w.state_hash();
    for cut in [1, 4, 12] {
        assert!(w.load_state(&bytes[..bytes.len() - cut]).is_err());
        assert_eq!(w.state_hash(), hash);
    }
    let mut corrupt = bytes.clone();
    let last = corrupt.len() - 5;
    corrupt[last] ^= 1;
    let mut changed = world(1);
    changed.load_state(&corrupt).unwrap();
    assert_ne!(changed.state_hash(), hash);
    assert!(w.add_item_notified(id(FIRST), id(LOOSE), 1, &mut alloc));
    let copy = w.reference_ref(id(FIRST)).unwrap().inventory.clone();
    w.reference(id(SECOND)).unwrap().inventory = copy;
    let mut loaded = world(1);
    let hash = loaded.state_hash();
    assert!(loaded.load_state(&w.save_state()).is_err());
    assert_eq!(loaded.state_hash(), hash);
}

#[test]
fn container_handle_extra_does_not_claim_the_item_reference_identity() {
    let stack = Stack::new(
        1,
        vec![Extra {
            kind: 28,
            fields: vec![Field::Form(Some(id(LOOSE)))],
        }],
    )
    .unwrap();
    assert_eq!(stack.reference(), None);
}

#[test]
fn papyrus_add_item_preserves_reference_identity_and_rejects_explicit_zero_count() {
    use soe_game::Game;
    use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallMethod, Return};

    let mut game = Game::new(order(1));
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("holder", "ObjectReference")
        .param("item", "Form")
        .param("count", "Int")
        .local("renamed", "Bool")
        .ins(
            CallMethod,
            vec![
                s("SetDisplayName"),
                ident("item"),
                ident("renamed"),
                s("Renamed blade"),
            ],
        )
        .ins(
            CallMethod,
            vec![s("SetActorOwner"), ident("item"), none(), none()],
        )
        .ins(
            CallMethod,
            vec![
                s("AddItem"),
                ident("holder"),
                none(),
                ident("item"),
                ident("count"),
            ],
        )
        .ins(Return, vec![none()])
        .build();
    game.load_script(
        &ScriptBuilder::new("InventoryProbe", "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let holder = Value::Object(game.object_of_form(id(FIRST)));
    let item = Value::Object(game.object_of_form(id(LOOSE)));
    for count in [0, 2, 1] {
        game.vm
            .call_global(
                "InventoryProbe",
                "Run",
                &[holder.clone(), item.clone(), Value::Int(count)],
            )
            .unwrap();
        game.run_until_idle(64);
        assert_eq!(
            game.world_mut().item_count(id(FIRST), id(ITEM)),
            i32::from(count == 1)
        );
    }
    assert_eq!(
        game.world()
            .reference_ref(id(FIRST))
            .unwrap()
            .inventory
            .reference_count(id(LOOSE)),
        1
    );
    let w = game.world();
    let stack = &w
        .reference_ref(id(FIRST))
        .unwrap()
        .inventory
        .stacks(id(ITEM))[0];
    assert!(stack.extra(33).is_none());
    assert_eq!(
        stack.extra(153).unwrap().fields.last(),
        Some(&Field::Text(b"Renamed blade".to_vec()))
    );
}

#[test]
fn legacy_contained_identity_does_not_recreate_its_authored_loose_item() {
    for materialized in [false, true] {
        let mut w = world(1);
        let mut alloc = allocator();
        if materialized {
            w.reference(id(LOOSE)).unwrap().item = None;
        }
        let stack = Stack::new(
            1,
            vec![Extra {
                kind: 32,
                fields: vec![Field::Form(Some(id(LOOSE)))],
            }],
        )
        .unwrap();
        w.reference(id(FIRST))
            .unwrap()
            .inventory
            .restore(id(ITEM), 1, vec![stack])
            .unwrap();
        let mut bytes = w.save_state();
        bytes.truncate(bytes.len() - 8);
        bytes[4..6].copy_from_slice(&25_u16.to_le_bytes());
        let mut loaded = world(1);
        loaded.load_state(&bytes).unwrap();
        assert!(loaded.reference_ref(id(LOOSE)).unwrap().item.is_none());
        assert_eq!(loaded.remove_item(id(FIRST), id(ITEM), 1, None), 1);
        assert!(!loaded.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
        let bytes = loaded.save_state();
        loaded.load_state(&bytes).unwrap();
        assert!(!loaded.add_item_notified(id(SECOND), id(LOOSE), 1, &mut alloc));
    }
}

#[test]
fn imported_contained_reference_materializes_without_losing_identity() {
    for reference in [
        id(LOOSE),
        // The created namespace is its own sentinel, not load-order slot 255.
        GlobalFormId {
            file: soe_game::world::DYNAMIC_FILE,
            object: 0xf000,
        },
    ] {
        let mut w = world(1);
        let mut alloc = allocator();
        let stack = Stack::new(
            1,
            vec![Extra {
                kind: 32,
                fields: vec![Field::Form(Some(reference))],
            }],
        )
        .unwrap();
        w.reference(id(FIRST))
            .unwrap()
            .inventory
            .restore(id(ITEM), 1, vec![stack])
            .unwrap();
        let mut loaded = world(1);
        loaded.load_state(&w.save_state()).unwrap();
        assert!(loaded.reference_ref(reference).unwrap().item.is_none());
        assert!(w.add_item_notified(id(SECOND), reference, 1, &mut alloc));
        loaded.load_state(&w.save_state()).unwrap();
        assert_eq!(loaded.state_hash(), w.state_hash());
        assert_eq!(loaded.remove_item(id(SECOND), id(ITEM), 1, None), 1);
        assert!(!loaded.add_item_notified(id(FIRST), reference, 1, &mut alloc));
        if reference.file == 0xff {
            assert!(loaded.place_at(id(ITEM), id(FIRST), false).object > reference.object);
        }
    }
}
