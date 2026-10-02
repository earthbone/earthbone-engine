use super::*;

const PARALYSIS_EFFECT: u32 = EFFECT + 10;
const OTHER_EFFECT: u32 = EFFECT + 11;
const CURE_EFFECT: u32 = EFFECT + 12;
const PARALYSIS_SPELL: u32 = SPELL + 10;
const OTHER_SPELL: u32 = SPELL + 11;
const CURE_SPELL: u32 = SPELL + 12;

fn cure_fixture() -> Plugin {
    let mut paralysis = effect(1 << 1, 21, 24, false);
    paralysis.form_id = PARALYSIS_EFFECT;
    let mut unrelated = effect(0, 1, u32::MAX, false);
    unrelated.form_id = OTHER_EFFECT;
    let mut cure = effect(0, 27, u32::MAX, false);
    cure.form_id = CURE_EFFECT;
    let mut paralysis_spell = spell(PARALYSIS_SPELL, 0, false, 1.0, 30);
    paralysis_spell
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = PARALYSIS_EFFECT.to_le_bytes().to_vec();
    let mut unrelated_spell = spell(OTHER_SPELL, 0, false, 1.0, 30);
    unrelated_spell
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = OTHER_EFFECT.to_le_bytes().to_vec();
    let mut cure_spell = spell(CURE_SPELL, 0, false, 0.0, 0);
    cure_spell
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = CURE_EFFECT.to_le_bytes().to_vec();
    let mut fixture = plugin(
        paralysis,
        vec![paralysis_spell, unrelated_spell, cure_spell],
    );
    fixture.push_top(unrelated);
    fixture.push_top(cure);
    fixture
}

#[test]
fn mag_48_cure_paralysis_removes_only_paralysis_effects_on_its_target() {
    let mut game = game(cure_fixture());
    let target_paralysis = cast(&mut game, PARALYSIS_SPELL, ACTOR).unwrap()[0];
    let other_paralysis = cast(&mut game, PARALYSIS_SPELL, OTHER).unwrap()[0];
    let unrelated = cast(&mut game, OTHER_SPELL, ACTOR).unwrap()[0];

    cast(&mut game, CURE_SPELL, ACTOR).unwrap();

    assert!(
        !game
            .world()
            .active_effects()
            .any(|active| active.id == target_paralysis)
    );
    assert!(
        game.world()
            .active_effects()
            .any(|active| active.id == other_paralysis)
    );
    assert!(
        game.world()
            .active_effects()
            .any(|active| active.id == unrelated)
    );
}

#[test]
fn mag_48_cure_paralysis_restores_the_targets_control_state() {
    let mut game = game(cure_fixture());
    cast(&mut game, PARALYSIS_SPELL, ACTOR).unwrap();
    assert!(game.world().actor_is_paralyzed(id(ACTOR)));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "paralysis"), 1.0);

    cast(&mut game, CURE_SPELL, ACTOR).unwrap();

    assert!(!game.world().actor_is_paralyzed(id(ACTOR)));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "paralysis"), 0.0);
}

#[test]
fn mag_48_ingestible_cure_paralysis_ends_live_paralysis() {
    let mut fixture = cure_fixture();
    let mut potion = ingestible(POTION, 0.0, 0);
    potion
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"EFID"))
        .unwrap()
        .data = CURE_EFFECT.to_le_bytes().to_vec();
    fixture.push_top(potion);
    let mut game = game(fixture);
    cast(&mut game, PARALYSIS_SPELL, ACTOR).unwrap();
    game.world_mut().add_item(id(ACTOR), id(POTION), 1);

    let shared = game.shared_world();
    shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();

    assert!(!game.world().actor_is_paralyzed(id(ACTOR)));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "paralysis"), 0.0);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(POTION)), 0);
}
