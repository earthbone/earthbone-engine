// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::{FormId, FormType};
use soe_commonlib_shim::world::{
    IFormFactory, InventoryChanges, InventoryEntryData, ItemList, Unsupported,
};

#[test]
fn absent_inventory_count_is_zero_and_failed_mutation_preserves_rows() {
    let object = FormId(0x0100_0001);
    let entry = InventoryEntryData {
        object,
        count_delta: 2,
        extra_types: vec![22],
        worn: true,
    };
    let mut inventory = InventoryChanges {
        owner: FormId(0x0100_0002),
        entries: vec![entry.clone()],
        totals: vec![(object, 2)],
    };
    let before = inventory.clone();
    assert_eq!(inventory.get_item_count(FormId(99)), 0);
    assert_eq!(
        inventory.add_entry_data(entry),
        Err(Unsupported::InventoryMutation)
    );
    assert_eq!(inventory, before);
    assert_eq!(
        ItemList {
            items: inventory.entries.clone()
        }
        .get_selected_item(),
        Err(Unsupported::ItemSelection)
    );
}

#[test]
fn factory_creation_and_empty_item_list_update_are_explicitly_unsupported() {
    let factory = IFormFactory {
        name: "WEAP".into(),
        form_type: FormType(*b"WEAP"),
    };
    assert_eq!(factory.create(), Err(Unsupported::FormCreation));
    let mut list = ItemList { items: vec![] };
    assert_eq!(list.update(), Err(Unsupported::ItemListUpdate));
    assert!(list.items.is_empty());
}
