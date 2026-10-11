// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::scr_13_actor_native_batch::{call, game, object};
use soe_esm::GlobalFormId;
use soe_game::world::MenuRequest;
use soe_papyrus::Value;

#[test]
fn show_barter_menu_queues_only_valid_calls_and_keeps_call_order() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    let guard_object = object(&mut game, guard);
    let criminal_object = object(&mut game, criminal);

    call(
        &mut game,
        guard_object.clone(),
        "ShowBarterMenu",
        &[Value::Bool(true)],
    );
    call(&mut game, guard_object.clone(), "ShowBarterMenu", &[]);
    call(&mut game, criminal_object.clone(), "ShowBarterMenu", &[]);

    assert_eq!(
        game.world().menu_requests,
        vec![MenuRequest::Barter(guard), MenuRequest::Barter(criminal)]
    );
    assert_eq!(game.traces().len(), 1);
    assert!(game.traces()[0].contains("ShowBarterMenu expects no arguments"));
}

#[test]
fn trespass_alarm_records_immediately_and_rejects_invalid_criminal_without_mutation() {
    let mut game = game();
    let witness = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    let witness_object = object(&mut game, witness);
    let criminal_object = object(&mut game, criminal);
    let faction = GlobalFormId {
        file: 0,
        object: 0x701,
    };

    call(
        &mut game,
        witness_object.clone(),
        "SendTrespassAlarm",
        std::slice::from_ref(&criminal_object),
    );
    assert_eq!(
        game.world_mut()
            .faction(faction)
            .unwrap()
            .crime_gold_nonviolent,
        75
    );
    assert!(game
        .world_mut()
        .actor(witness)
        .unwrap()
        .witnessed_crimes
        .contains(&criminal));

    call(
        &mut game,
        witness_object,
        "SendTrespassAlarm",
        &[Value::None],
    );
    assert_eq!(
        game.world_mut()
            .faction(faction)
            .unwrap()
            .crime_gold_nonviolent,
        75
    );
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("akCriminal must be an Actor")));
}

#[test]
fn stop_combat_alarm_clears_inbound_combat_but_not_other_actor_alarm() {
    let mut game = game();
    let receiver = game.form("GuardRef").unwrap();
    let opponent = game.form("CriminalRef").unwrap();
    {
        let mut world = game.world_mut();
        let receiver_state = world.actor(receiver).unwrap();
        receiver_state.in_combat = true;
        receiver_state.combat_target = Some(opponent);
        receiver_state.alerted = true;
        let opponent_state = world.actor(opponent).unwrap();
        opponent_state.in_combat = true;
        opponent_state.combat_target = Some(receiver);
        opponent_state.alerted = true;
    }

    let receiver_object = object(&mut game, receiver);
    assert_eq!(
        call(&mut game, receiver_object, "StopCombatAlarm", &[]),
        Value::None
    );
    let mut world = game.world_mut();
    for id in [receiver, opponent] {
        let state = world.actor(id).unwrap();
        assert!(!state.in_combat);
        assert_eq!(state.combat_target, None);
        assert!(!state.alerted);
    }
}

#[test]
fn inventory_calls_queue_once_per_eligible_invocation_in_order() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    game.world_mut().actor(guard).unwrap().teammate = true;
    let guard_object = object(&mut game, guard);
    let criminal_object = object(&mut game, criminal);

    call(&mut game, guard_object.clone(), "OpenInventory", &[]);
    call(
        &mut game,
        criminal_object.clone(),
        "OpenInventory",
        &[Value::Bool(false)],
    );
    call(
        &mut game,
        criminal_object,
        "OpenInventory",
        &[Value::Bool(true)],
    );

    assert_eq!(
        game.world().menu_requests,
        vec![
            MenuRequest::ActorInventory(guard),
            MenuRequest::ActorInventory(criminal)
        ]
    );
}

#[test]
fn clear_forced_movement_is_idempotent_and_retains_assigned_package() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let package = soe_esm::GlobalFormId {
        file: 0,
        object: 0x777,
    };
    {
        let mut world = game.world_mut();
        let ai = &mut world.actor(guard).unwrap().ai;
        ai.package = Some(package);
        ai.destination = Some([100.0, 0.0, 0.0]);
        ai.path = vec![[10.0, 0.0, 0.0]];
        ai.path_cells = vec![None];
        ai.arrived = false;
        ai.reevaluate = false;
    }
    let target = object(&mut game, guard);

    call(&mut game, target.clone(), "ClearForcedMovement", &[]);
    call(&mut game, target, "ClearForcedMovement", &[]);

    let mut world = game.world_mut();
    let ai = &world.actor(guard).unwrap().ai;
    assert_eq!(ai.package, Some(package));
    assert_eq!(ai.destination, None);
    assert!(ai.path.is_empty());
    assert!(ai.path_cells.is_empty());
    assert!(ai.arrived);
    assert!(ai.reevaluate);
}
