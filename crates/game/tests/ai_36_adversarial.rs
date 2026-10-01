#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("support/ai_36_use_weapon.rs");

#[test]
fn ai_36_completed_useweapon_does_not_repeat_after_world_save_load() {
    let mut game = game();
    game.tick(0.1);
    let health = game.world_mut().actor_value(id(TARGET), "health");
    let saved = game.world().save_state();

    game.world_mut().load_state(&saved).unwrap();
    game.tick(0.1);

    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), health);
    assert!(
        game.world()
            .actor_ref(id(ATTACKER))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
}
