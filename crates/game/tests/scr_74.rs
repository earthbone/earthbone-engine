// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_papyrus::Value;

use crate::natives_dlc::{call, form, game, obj};

const GAME_DOC: &str = include_str!("../../../docs/game.md");

#[test]
fn scr_74_game_docs_equip_item_contract_covers_both_arguments() {
    let contract = GAME_DOC
        .lines()
        .find(|line| line.starts_with("| Actor.EquipItem |"))
        .expect("Actor.EquipItem contract row is present");

    for term in [
        "`abPreventRemoval` prevents replacement through EquipItem",
        "`abSilent` suppresses the equip message",
        "rejects replacement and direct unequip",
        "`UnequipAll` and `RemoveAllItems` clear the protection",
        "survives save/load in the `SCR-74` actor section",
        "queues an `Equipped <name>` notification unless silent",
    ] {
        assert!(contract.contains(term), "missing contract detail: {term}");
    }
    assert!(
        !contract.contains("ignored"),
        "the SCR-74 contract must not describe the EquipItem arguments as ignored"
    );
}

#[test]
fn scr_74_prevent_removal_blocks_slot_replacement_and_manual_unequip() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let actor_id = form(&game, "PlayerRef");
    let protected = form(&game, "FarmClothes");
    let replacement = obj(&mut game, "VampireRobes");
    let replacement_id = form(&game, "VampireRobes");
    let protected_object = obj(&mut game, "FarmClothes");

    call(
        &mut game,
        &player,
        "EquipItem",
        &[protected_object.clone(), Value::Bool(true)],
    );
    call(
        &mut game,
        &player,
        "EquipItem",
        &[replacement.clone(), Value::Bool(false)],
    );
    call(&mut game, &player, "UnequipItem", &[protected_object]);

    let mut world = game.world_mut();
    let actor = world.actor(actor_id).unwrap();
    assert!(actor.equipped.contains(&protected));
    assert!(actor.prevent_equipment_removal.contains(&protected));
    assert!(!actor.equipped.contains(&replacement_id));
}

#[test]
fn scr_74_unequip_all_and_remove_all_items_clear_protected_equipment() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let actor_id = form(&game, "PlayerRef");
    let protected = form(&game, "FarmClothes");
    let protected_object = obj(&mut game, "FarmClothes");

    call(
        &mut game,
        &player,
        "EquipItem",
        &[protected_object, Value::Bool(true)],
    );
    call(&mut game, &player, "UnequipAll", &[]);
    assert!(!game
        .world_mut()
        .actor(actor_id)
        .unwrap()
        .prevent_equipment_removal
        .contains(&protected));
    assert!(!game
        .world_mut()
        .actor(actor_id)
        .unwrap()
        .equipped
        .contains(&protected));

    let protected_object = obj(&mut game, "FarmClothes");
    call(
        &mut game,
        &player,
        "EquipItem",
        &[protected_object, Value::Bool(true)],
    );
    call(
        &mut game,
        &player,
        "RemoveAllItems",
        &[Value::None, Value::Bool(false)],
    );
    {
        let mut world = game.world_mut();
        let actor = world.actor(actor_id).unwrap();
        assert!(!actor.prevent_equipment_removal.contains(&protected));
        assert!(!actor.equipped.contains(&protected));
    }
    assert_eq!(game.world_mut().item_count(actor_id, protected), 0);
}

#[test]
fn scr_74_prevent_removal_survives_save_round_trip() {
    let mut source = game();
    let player = obj(&mut source, "PlayerRef");
    let actor_id = form(&source, "PlayerRef");
    let protected = form(&source, "FarmClothes");
    let protected_object = obj(&mut source, "FarmClothes");
    call(
        &mut source,
        &player,
        "EquipItem",
        &[protected_object, Value::Bool(true)],
    );

    let bytes = source.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&bytes).unwrap();
    let player = obj(&mut restored, "PlayerRef");
    let replacement_object = obj(&mut restored, "VampireRobes");
    let replacement = form(&restored, "VampireRobes");
    call(
        &mut restored,
        &player,
        "EquipItem",
        &[replacement_object, Value::Bool(false)],
    );

    let mut world = restored.world_mut();
    let actor = world.actor(actor_id).unwrap();
    assert!(actor.equipped.contains(&protected));
    assert!(actor.prevent_equipment_removal.contains(&protected));
    assert!(!actor.equipped.contains(&replacement));
}

#[test]
fn scr_74_silent_suppresses_equip_notification() {
    let mut game = game();
    let player = obj(&mut game, "PlayerRef");
    let silent_item = obj(&mut game, "WarmArmor");
    let regular_item = obj(&mut game, "ColdArmor");
    call(
        &mut game,
        &player,
        "EquipItem",
        &[silent_item, Value::Bool(false), Value::Bool(true)],
    );
    assert!(game.world().notifications.is_empty());

    call(
        &mut game,
        &player,
        "EquipItem",
        &[regular_item, Value::Bool(false), Value::Bool(false)],
    );
    assert_eq!(game.world().notifications.len(), 1);
    assert!(game.world().notifications[0].contains("ColdArmor"));
}
