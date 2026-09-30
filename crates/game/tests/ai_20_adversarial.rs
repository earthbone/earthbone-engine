#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("support/ai_20_sit.rs");

#[test]
fn ai_20_restored_sit_finishes_and_releases_its_marker() {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    let saved = game.world().save_state();

    let mut restored = World::new(order());
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.furniture_seat(id(ACTOR)), Some((id(CHAIR), 0)));
    let mut resumed = Game::new(order());
    resumed.world_mut().load_state(&saved).unwrap();
    resumed.tick(2.0);

    assert_eq!(resumed.world().furniture_seat(id(ACTOR)), None);
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
