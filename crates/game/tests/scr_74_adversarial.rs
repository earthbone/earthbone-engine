// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::Value;

use crate::natives_dlc::{call, form, game, obj};

#[test]
fn scr_74_protection_does_not_block_transfer_to_container() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let vampire = obj(&mut game, "VampireRef");
    let player_id = form(&game, "PlayerRef");
    let vampire_id = form(&game, "VampireRef");
    let armor_id = form(&game, "FarmClothes");
    let armor = obj(&mut game, "FarmClothes");

    call(
        &mut game,
        &player,
        "EquipItem",
        &[armor.clone(), Value::Bool(true)],
    );
    call(
        &mut game,
        &player,
        "RemoveItem",
        &[armor, Value::Int(1), Value::Bool(false), vampire],
    );

    let mut world = game.world_mut();
    assert_eq!(world.item_count(player_id, armor_id), 0);
    assert_eq!(world.item_count(vampire_id, armor_id), 1);
    let actor = world.actor(player_id).unwrap();
    assert!(!actor.equipped.contains(&armor_id));
    assert!(!actor.prevent_equipment_removal.contains(&armor_id));
}

#[test]
fn scr_74_default_arguments_allow_removal_and_show_equip_message() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let player_id = form(&game, "PlayerRef");
    let armor_id = form(&game, "WarmArmor");
    let armor = obj(&mut game, "WarmArmor");

    call(&mut game, &player, "EquipItem", &[armor]);
    assert!(game.world().notifications.iter().any(|text| text.contains("WarmArmor")));
    let armor = obj(&mut game, "WarmArmor");
    call(&mut game, &player, "UnequipItem", &[armor]);

    let mut world = game.world_mut();
    let actor = world.actor(player_id).unwrap();
    assert!(!actor.equipped.contains(&armor_id));
    assert!(!actor.prevent_equipment_removal.contains(&armor_id));
}
