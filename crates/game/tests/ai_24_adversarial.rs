#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

include!("support/ai_24_use_magic.rs");

#[test]
fn ai_24_completed_use_magic_does_not_cast_again_on_the_next_tick() {
    let mut game = fixture(0, 0);
    game.tick(0.1);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 105.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 13.0);

    game.tick(0.1);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "health"), 105.0);
    assert_eq!(game.world_mut().actor_value(id(CASTER), "magicka"), 13.0);
}
