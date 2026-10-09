// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_papyrus::Value;

use crate::natives_dlc::{call, form, game, obj};

#[test]
fn equip_item_grants_a_missing_item_once_across_repeated_calls() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let player_id = form(&game, "PlayerRef");
    let armor_id = form(&game, "WarmArmor");

    for _ in 0..2 {
        let armor = obj(&mut game, "WarmArmor");
        assert_eq!(call(&mut game, &player, "EquipItem", &[armor]), Value::None);
    }

    let mut world = game.world_mut();
    assert_eq!(world.item_count(player_id, armor_id), 1);
    let actor = world.actor(player_id).unwrap();
    assert!(actor.equipped.contains(&armor_id));
    assert!(!actor.prevent_equipment_removal.contains(&armor_id));
    drop(world);
    assert_eq!(game.world().notifications.len(), 1);
}
