#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("support/ai_29_shout.rs");

#[test]
fn ai_29_active_shout_recovery_resumes_without_recasting_after_save_load() {
    let mut game = game();
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 90.0);
    let saved = game.world().save_state();

    let mut restored = soe_game::world::World::new(load_order(3));
    restored.load_state(&saved).unwrap();
    let mut resumed = Game::new(load_order(3));
    resumed.world_mut().load_state(&saved).unwrap();
    resumed.tick(4.0);

    assert_eq!(resumed.world_mut().actor_value(id(ACTOR), "health"), 90.0);
    assert_eq!(resumed.world_mut().voice_recovery_time(id(ACTOR)), 0.0);
    assert!(
        resumed
            .world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
}
