use super::*;
use soe_game::combat::Hand;

fn equipped_game() -> Game {
    let mut cast = spell(SPELL, 0, false, 2.0, 5);
    let spit = cast
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .expect("spell SPIT fixture");
    put(&mut spit.data, 0, 10);
    put(&mut spit.data, 4, 1);
    put(&mut spit.data, 16, 1);
    put(&mut spit.data, 20, 0);
    let mut magic_effect = effect(1 << 1, 0, 24, false);
    let data = magic_effect
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .expect("effect DATA fixture");
    put(&mut data.data, 84, 0);
    let game = game(plugin(magic_effect, vec![cast]));
    let actor = id(ACTOR);
    game.world_mut()
        .actor(actor)
        .unwrap()
        .spells
        .insert(id(SPELL));
    game.world_mut()
        .equip_hand_spell(actor, Hand::Left, id(SPELL))
        .unwrap();
    game.world_mut()
        .equip_hand_spell(actor, Hand::Right, id(SPELL))
        .unwrap();
    game
}

#[test]
fn mag_22_each_hand_begins_once_and_release_allows_the_next_cast() {
    let mut game = equipped_game();
    let actor = id(ACTOR);

    assert!(game.begin_hand_cast(actor, Hand::Left, None).unwrap());
    assert!(!game.begin_hand_cast(actor, Hand::Left, None).unwrap());
    assert!(game.end_hand_cast(actor, Hand::Left));
    assert!(!game.end_hand_cast(actor, Hand::Left));
    assert!(game.begin_hand_cast(actor, Hand::Left, None).unwrap());
    assert!(game.end_hand_cast(actor, Hand::Left));

    assert!(game.begin_hand_cast(actor, Hand::Right, None).unwrap());
    assert!(!game.begin_hand_cast(actor, Hand::Right, None).unwrap());
    assert!(game.end_hand_cast(actor, Hand::Right));

    assert_eq!(game.world_mut().actor_value(actor, "magicka"), 70.0);
    assert_eq!(game.world().active_effects().count(), 3);
}
