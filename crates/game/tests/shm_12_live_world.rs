// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{ReferenceKind, Unsupported};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, commonlib};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_reference() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MISC"), 0x100));
    plugin.push_top(Record::new(T(b"REFR"), 0x200).sub(T(b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("ShimFixture.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn shm_12_factory_describes_loaded_record_type_and_rejects_allocation() {
    let game = game_with_reference();
    let world = game.world();
    let factory = commonlib::form_factory(&world, *b"MISC").unwrap();
    assert_eq!(factory.get_form_type().as_str(), "MISC");
    assert_eq!(factory.get_form_name(), "MISC");
    assert_eq!(factory.create().unwrap_err(), Unsupported::FormCreation);
    assert!(commonlib::form_factory(&world, *b"XXXX").is_err());
}

#[test]
fn shm_12_inventory_changes_and_entry_data_read_live_stacks() {
    let game = game_with_reference();
    let owner = id(0x200);
    game.world_mut().add_item(owner, id(0x100), 5);
    let mut changes = commonlib::inventory_changes(&mut game.world_mut(), owner).unwrap();
    assert_eq!(changes.owner, FormId(0x0000_0200));
    assert_eq!(changes.get_item_count(FormId(0x0000_0100)), 5);
    assert_eq!(changes.entries.len(), 1);
    assert_eq!(changes.entries[0].count_delta, 5);
    assert!(!changes.entries[0].worn);
    assert_eq!(
        changes
            .add_entry_data(changes.entries[0].clone())
            .unwrap_err(),
        Unsupported::InventoryMutation
    );
}

#[test]
fn shm_12_item_list_projects_inventory_and_rejects_menu_selection() {
    let game = game_with_reference();
    let owner = id(0x200);
    game.world_mut().add_item(owner, id(0x100), 2);
    let items = commonlib::item_list(&mut game.world_mut(), owner).unwrap();
    assert_eq!(items.items.len(), 1);
    assert_eq!(items.items[0].count_delta, 2);
    assert_eq!(
        items.get_selected_item().unwrap_err(),
        Unsupported::ItemSelection
    );
    let mut items = items;
    assert_eq!(items.update().unwrap_err(), Unsupported::ItemListUpdate);
}

#[test]
fn shm_12_reference_handle_resolves_reference_and_actor_state() {
    let mut game = game_with_reference();
    let reference =
        commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x200)).unwrap();
    assert_eq!(reference.form_id, FormId(0x200));
    assert_eq!(reference.kind, ReferenceKind::Reference);

    let player = game.player();
    let player_handle = FormId(game.world().runtime_id(player));
    let actor =
        commonlib::lookup_reference_by_handle(&mut game.world_mut(), player_handle).unwrap();
    assert_eq!(actor.kind, ReferenceKind::Actor);
    assert!(commonlib::lookup_reference_by_handle(&mut game.world_mut(), FormId(0x1234)).is_err());
}
