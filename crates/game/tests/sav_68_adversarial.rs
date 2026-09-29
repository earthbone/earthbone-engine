// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use std::path::PathBuf;

const ACTOR: u32 = 0x202;
const ITEM: u32 = 0x301;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x302));
    plugin.push_top(Record::new(Tag(*b"MISC"), ITEM));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), 0x302_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("ResetActor.esm", plugin).unwrap();
    order
}

#[test]
fn sav_68_reset_keeps_actor_factions_and_relationships_after_save_load() {
    let mut world = World::new(fixture_order());
    let actor_id = id(ACTOR);
    let other = id(0x200);
    world.actor(actor_id).unwrap().factions.insert(id(ITEM), 2);
    world
        .actor(actor_id)
        .unwrap()
        .relationships
        .insert(other, 3);
    world.reset_reference(actor_id, None);
    let saved = world.save_state();

    let mut restored = World::new(fixture_order());
    restored.load_state(&saved).unwrap();
    let actor = restored.actor(actor_id).unwrap();
    assert_eq!(actor.factions.get(&id(ITEM)), Some(&2));
    assert_eq!(actor.relationships.get(&other), Some(&3));
}

#[test]
fn sav_68_changed_vanilla_container_resets_to_winning_record_after_load() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed reset check");
        return;
    };
    let path = data.join("Skyrim.esm");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut world = World::new(order);
    let reference = world
        .order()
        .winners_of(Tag(*b"REFR"))
        .find_map(|(id, record)| {
            let base = record.find(Tag(*b"NAME"))?;
            let raw = u32::from_le_bytes(base.try_into().ok()?);
            let base_id = GlobalFormId {
                file: id.file,
                object: raw,
            };
            world
                .order()
                .winner(base_id)
                .filter(|base| base.tag == Tag(*b"CONT") && base.find(Tag(*b"CNTO")).is_some())
                .map(|_| id)
        })
        .expect("Skyrim.esm contains a placed container with base inventory");
    let authored = world.reference(reference).unwrap().clone();
    {
        let changed = world.reference(reference).unwrap();
        changed.position[0] += 256.0;
        changed.enabled = !changed.enabled;
        if let Some((item, count)) = authored.inventory.iter().next() {
            changed.inventory.take(*item, count);
        }
    }
    assert!(world.reset_reference(reference, None));
    let saved = world.save_state();

    let mut restored_order = LoadOrder::new();
    restored_order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut restored = World::new(restored_order);
    restored.load_state(&saved).unwrap();
    let reset = restored.reference_ref(reference).unwrap();
    assert_eq!(reset.position, authored.position);
    assert_eq!(reset.enabled, authored.enabled);
    assert_eq!(reset.inventory, authored.inventory);
}
