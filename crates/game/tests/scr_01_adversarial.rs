// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-01 inventory-filter boundary checks against the ActiveMagicEffect PSC.

use soe_esm::GlobalFormId;
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_01_filter_remove_and_save_load_preserve_listener_scoping() {
    let listener = ObjectId(55);
    let other_listener = ObjectId(56);
    let first = id(1);
    let second = id(2);
    let mut game = Game::new(soe_esm::LoadOrder::new());
    {
        let mut world = game.world_mut();
        world.add_inventory_event_filter(listener, first);
        world.add_inventory_event_filter(listener, second);
        world.add_inventory_event_filter(other_listener, first);
        world.remove_inventory_event_filter(listener, Some(first));
        assert_eq!(world.inventory_filters[&listener], vec![second]);
        assert_eq!(world.inventory_filters[&other_listener], vec![first]);
        world.remove_all_inventory_event_filters(listener);
        assert!(!world.inventory_filters.contains_key(&listener));
    }
    let saved = game.world_mut().save_state();
    let loaded = Game::new(soe_esm::LoadOrder::new());
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        loaded.world().inventory_filters[&other_listener],
        vec![first]
    );
}
