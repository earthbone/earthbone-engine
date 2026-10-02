// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "scr_13_actor_native_batch.rs"]
mod batch;

use batch::{call, game as fixture, object};
use soe_esm::GlobalFormId;
use soe_papyrus::Value;

#[test]
fn scr_13_stop_combat_alarm_stops_actors_fighting_the_receiver() {
    let mut game = fixture();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    {
        let mut world = game.world_mut();
        let opponent = world.actor(criminal).unwrap();
        opponent.in_combat = true;
        opponent.combat_target = Some(guard);
        opponent.alerted = true;
    }

    let receiver = object(&mut game, guard);
    call(&mut game, receiver, "StopCombatAlarm", &[]);

    let mut world = game.world_mut();
    let opponent = world.actor(criminal).unwrap();
    assert!(!opponent.in_combat, "combat against receiver must stop");
    assert_eq!(opponent.combat_target, None);
    assert!(!opponent.alerted, "alarm against receiver must stop");
}

#[test]
fn scr_13_trespass_alarm_repeats_and_witness_state_survives_save_load() {
    let mut game = fixture();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    let witness = object(&mut game, guard);
    let perpetrator = object(&mut game, criminal);

    call(
        &mut game,
        witness.clone(),
        "SendTrespassAlarm",
        std::slice::from_ref(&perpetrator),
    );
    call(
        &mut game,
        witness,
        "SendTrespassAlarm",
        std::slice::from_ref(&perpetrator),
    );
    assert_eq!(
        game.world_mut()
            .faction(GlobalFormId { file: 0, object: 0x701 })
            .unwrap()
            .crime_gold_nonviolent,
        150,
        "each explicit alarm reports the trespass again"
    );

    let bytes = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&bytes).unwrap();
    assert!(restored
        .world_mut()
        .actor(guard)
        .unwrap()
        .witnessed_crimes
        .contains(&criminal));
    assert_eq!(
        restored
            .world_mut()
            .faction(GlobalFormId { file: 0, object: 0x701 })
            .unwrap()
            .crime_gold_nonviolent,
        150
    );
}

#[test]
fn scr_13_trespass_alarm_without_crime_faction_reports_error_without_witness_state() {
    let mut game = fixture();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    game.world_mut().actor(criminal).unwrap().crime_faction = None;

    let witness = object(&mut game, guard);
    let perpetrator = object(&mut game, criminal);
    call(
        &mut game,
        witness,
        "SendTrespassAlarm",
        &[perpetrator],
    );

    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("has no crime faction")));
    assert!(game
        .world_mut()
        .actor(guard)
        .unwrap()
        .witnessed_crimes
        .is_empty());
}

#[test]
fn scr_13_show_barter_menu_rejects_argument_and_preserves_one_request_per_call() {
    let mut game = fixture();
    let guard = game.form("GuardRef").unwrap();
    let target = object(&mut game, guard);

    call(
        &mut game,
        target.clone(),
        "ShowBarterMenu",
        &[Value::Bool(false)],
    );
    call(&mut game, target.clone(), "ShowBarterMenu", &[]);
    call(&mut game, target, "ShowBarterMenu", &[]);

    assert_eq!(
        game.world().menu_requests,
        vec![
            soe_game::world::MenuRequest::Barter(guard),
            soe_game::world::MenuRequest::Barter(guard)
        ]
    );
    assert_eq!(game.traces().len(), 1);
}
