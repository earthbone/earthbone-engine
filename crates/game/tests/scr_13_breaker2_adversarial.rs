// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::scr_13_actor_native_batch::{call, game, object};

#[test]
fn stop_combat_alarm_stops_combat_and_alerts_against_the_receiving_actor() {
    let mut game = game();
    let receiver = game.form("GuardRef").unwrap();
    let attacker = game.form("CriminalRef").unwrap();
    {
        let mut world = game.world_mut();
        let state = world.actor(attacker).unwrap();
        state.in_combat = true;
        state.combat_target = Some(receiver);
        state.alerted = true;
    }
    let receiver_object = object(&mut game, receiver);
    call(&mut game, receiver_object, "StopCombatAlarm", &[]);
    let mut world = game.world_mut();
    let state = world.actor(attacker).unwrap();
    assert!(
        !state.in_combat,
        "CK says combat against the receiver stops"
    );
    assert_eq!(state.combat_target, None);
    assert!(!state.alerted, "CK says alarms against the receiver stop");
}
