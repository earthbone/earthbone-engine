// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::Unsupported;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{commonlib, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x100));
    plugin.push_top(Record::new(T(b"MISC"), 0x101));
    plugin.push_top(Record::new(T(b"REFR"), 0x200).sub(T(b"NAME"), 0x100_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"REFR"), 0x201).sub(T(b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("ShimAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn shm_12_rejects_missing_inventory_owners_and_non_reference_handles() {
    let game = fixture();
    assert!(commonlib::inventory_changes(&mut game.world_mut(), id(0x999)).is_err());
    assert!(commonlib::item_list(&mut game.world_mut(), id(0x999)).is_err());
    assert!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x999)).is_err());
    assert!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x100)).is_err());
}

#[test]
fn shm_12_empty_inventory_is_readable_and_item_list_is_empty() {
    let game = fixture();
    let changes = commonlib::inventory_changes(&mut game.world_mut(), id(0x200)).unwrap();
    assert_eq!(changes.owner, FormId(0x200));
    assert!(changes.entries.is_empty());
    assert!(changes.totals.is_empty());
    let items = commonlib::item_list(&mut game.world_mut(), id(0x200)).unwrap();
    assert!(items.items.is_empty());
}

#[test]
fn shm_12_repeat_reads_preserve_inventory_and_sorted_item_order() {
    let game = fixture();
    let owner = id(0x200);
    game.world_mut().add_item(owner, id(0x101), 3);
    game.world_mut().add_item(owner, id(0x100), 2);

    let first = commonlib::inventory_changes(&mut game.world_mut(), owner).unwrap();
    let second = commonlib::inventory_changes(&mut game.world_mut(), owner).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.totals, vec![(FormId(0x100), 2), (FormId(0x101), 3)]);
    assert_eq!(
        first
            .entries
            .iter()
            .map(|entry| entry.object)
            .collect::<Vec<_>>(),
        vec![FormId(0x100), FormId(0x101)]
    );

    let items = commonlib::item_list(&mut game.world_mut(), owner).unwrap();
    assert_eq!(items.items, first.entries);
}

#[test]
fn shm_12_factory_and_read_only_hooks_remain_explicit_on_repeated_calls() {
    let game = fixture();
    let factory = commonlib::form_factory(&game.world(), *b"MISC").unwrap();
    assert_eq!(factory.get_form_type().as_str(), "MISC");
    assert_eq!(factory.create().unwrap_err(), Unsupported::FormCreation);
    assert_eq!(factory.create().unwrap_err(), Unsupported::FormCreation);
    assert!(commonlib::form_factory(&game.world(), *b"XXXX").is_err());

    let game = fixture();
    game.world_mut().add_item(id(0x200), id(0x100), 1);
    let mut changes = commonlib::inventory_changes(&mut game.world_mut(), id(0x200)).unwrap();
    let entry = changes.entries[0].clone();
    assert_eq!(
        changes.add_entry_data(entry.clone()).unwrap_err(),
        Unsupported::InventoryMutation
    );
    assert_eq!(
        changes.add_entry_data(entry).unwrap_err(),
        Unsupported::InventoryMutation
    );
    assert_eq!(changes.get_item_count(FormId(0x100)), 1);

    let mut items = commonlib::item_list(&mut game.world_mut(), id(0x200)).unwrap();
    assert_eq!(
        items.get_selected_item().unwrap_err(),
        Unsupported::ItemSelection
    );
    assert_eq!(items.update().unwrap_err(), Unsupported::ItemListUpdate);
    assert_eq!(items.items.len(), 1);
}
