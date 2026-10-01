// SPDX-License-Identifier: GPL-3.0-or-later

include!("scr_13_actor_native_batch.rs");

#[test]
fn scr_13_repeated_alarm_stop_is_idempotent_and_preserves_other_actor() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    {
        let mut world = game.world_mut();
        world.actor(guard).unwrap().in_combat = true;
        world.actor(guard).unwrap().combat_target = Some(criminal);
        world.actor(criminal).unwrap().alerted = true;
    }
    let guard_object = object(&mut game, guard);
    call(&mut game, guard_object.clone(), "StopCombatAlarm", &[]);
    call(&mut game, guard_object, "StopCombatAlarm", &[]);
    assert!(!game.world_mut().actor(guard).unwrap().in_combat);
    assert_eq!(game.world_mut().actor(guard).unwrap().combat_target, None);
    assert!(game.world_mut().actor(criminal).unwrap().alerted);
}
