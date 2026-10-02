use super::*;
use soe_game::inventory::StackSource;

fn scroll(form: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, 3);
    put(&mut data, 20, 3);
    data[28..32].copy_from_slice(&128.0_f32.to_le_bytes());
    let mut efit = 20.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    Record::new(T(b"SCRL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

#[test]
fn mag_18_successful_scroll_cast_consumes_one_selected_item() {
    let mut game = game(plugin(effect(1 << 1, 0, 24, false), vec![scroll(SPELL)]));
    game.world_mut().add_item(id(ACTOR), id(SPELL), 2);
    let selection = game
        .world_mut()
        .select_item(id(ACTOR), id(SPELL), StackSource::Plain, 1)
        .unwrap();
    let applied = game.shared_world().borrow_mut().cast_scroll(
        id(ACTOR),
        &selection,
        id(SOURCE),
        Some(id(ACTOR)),
        Some(id(ACTOR)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert_eq!(applied.unwrap().len(), 1);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(SPELL)), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 120.0);
    let source = game.object_of_form(id(SOURCE));
    assert!(game.world().pending.iter().any(|pending| {
        matches!(pending, Pending::Event { object, name, args }
            if *object == source && name == "OnSpellCast" && args.len() == 1)
    }));
}

#[test]
fn mag_18_scroll_script_flag_does_not_bypass_magic_resistance() {
    let mut definition = scroll(SPELL);
    put(&mut definition.subrecords[0].data, 4, 1 << 20);
    let mut game = game(plugin(effect_with_resistance(44), vec![definition]));
    game.world_mut()
        .set_actor_value(id(ACTOR), "resistmagic", 85.0)
        .unwrap();
    game.world_mut().add_item(id(ACTOR), id(SPELL), 1);
    let selection = game
        .world_mut()
        .select_item(id(ACTOR), id(SPELL), StackSource::Plain, 1)
        .unwrap();

    game.shared_world()
        .borrow_mut()
        .cast_scroll(
            id(ACTOR),
            &selection,
            id(SOURCE),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 97.0);
}

#[test]
fn mag_18_rejected_scroll_cast_keeps_selected_item_and_effects_unchanged() {
    let mut game = game(plugin(effect(1 << 1, 0, 24, false), vec![scroll(SPELL)]));
    game.world_mut().add_item(id(ACTOR), id(SPELL), 2);
    let selection = game
        .world_mut()
        .select_item(id(ACTOR), id(SPELL), StackSource::Plain, 1)
        .unwrap();
    let result = game.shared_world().borrow_mut().cast_scroll(
        id(ACTOR),
        &selection,
        id(SOURCE),
        Some(id(ACTOR)),
        Some(id(ITEM)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(matches!(result, Err(MagicError::Target)));
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(SPELL)), 2);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(
        !game
            .world()
            .pending
            .iter()
            .any(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnSpellCast"))
    );
}

#[test]
fn mag_18_concentration_effect_is_rejected_without_consuming_scroll() {
    let mut effect = effect(1 << 1, 0, 24, false);
    put(&mut effect.subrecords[1].data, 80, 2);
    let mut game = game(plugin(effect, vec![scroll(SPELL)]));
    game.world_mut().add_item(id(ACTOR), id(SPELL), 1);
    let selection = game
        .world_mut()
        .select_item(id(ACTOR), id(SPELL), StackSource::Plain, 1)
        .unwrap();
    let result = game.shared_world().borrow_mut().cast_scroll(
        id(ACTOR),
        &selection,
        id(SOURCE),
        Some(id(ACTOR)),
        Some(id(ACTOR)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(matches!(result, Err(MagicError::Unsupported(_))));
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(SPELL)), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}
