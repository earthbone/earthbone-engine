// SPDX-License-Identifier: GPL-3.0-or-later

//! Armor EITM links resolve to constant ENCH effects that end on unequip.

use super::*;

const ENCHANTMENT: u32 = 0x700;
const ARMOR: u32 = 0x701;
const REPLACEMENT_ENCHANTMENT: u32 = 0x702;
const REPLACEMENT_ARMOR: u32 = 0x703;

fn enchanted_armor_fixture() -> Plugin {
    let mut records = plugin(effect(1 << 1, 0, 24, true), vec![]);
    let mut enit = vec![0; 36];
    put(&mut enit, 20, 6);
    records.push_top(
        Record::new(T(b"ENCH"), ENCHANTMENT)
            .sub(T(b"ENIT"), enit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes().as_slice(),
                    &0u32.to_le_bytes(),
                    &0u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    records.push_top(
        Record::new(T(b"ARMO"), ARMOR)
            .sub(
                T(b"DATA"),
                [0_u32.to_le_bytes(), 1_f32.to_le_bytes()].concat(),
            )
            .sub(T(b"DNAM"), 100_u32.to_le_bytes())
            .sub(
                T(b"BOD2"),
                [1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
            )
            .sub(T(b"EITM"), ENCHANTMENT.to_le_bytes()),
    );
    records
}

#[test]
fn mag_10_equipping_armor_applies_its_enchantment_effect() {
    let mut game = game(enchanted_armor_fixture());
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    let world = game.world();
    let effect = world.active_effects().next().unwrap();
    assert_eq!(effect.spell, id(ENCHANTMENT));
    assert_eq!(effect.source, id(ARMOR));
    assert_eq!(effect.duration, None);
}

#[test]
fn mag_10_unequipping_armor_removes_its_enchantment_effect() {
    let mut game = game(enchanted_armor_fixture());
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    shared
        .borrow_mut()
        .unequip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_10_saved_worn_enchantment_restores_without_applying_twice() {
    let mut original = game(enchanted_armor_fixture());
    let shared = original.shared_world();
    shared
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            original.vm.create_object_of(name, class)
        })
        .unwrap();
    let saved = original.save_state().unwrap();

    let mut restored = game(enchanted_armor_fixture());
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    assert_eq!(restored.world().active_effects().count(), 1);

    let shared = restored.shared_world();
    shared
        .borrow_mut()
        .unequip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            restored.vm.create_object_of(name, class)
        })
        .unwrap();
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

#[test]
fn mag_10_replacing_armor_ends_the_old_enchantment() {
    let mut fixture = enchanted_armor_fixture();
    let mut enit = vec![0; 36];
    put(&mut enit, 20, 6);
    fixture.push_top(
        Record::new(T(b"ENCH"), REPLACEMENT_ENCHANTMENT)
            .sub(T(b"ENIT"), enit)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes().as_slice(),
                    &0u32.to_le_bytes(),
                    &0u32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    fixture.push_top(
        Record::new(T(b"ARMO"), REPLACEMENT_ARMOR)
            .sub(
                T(b"DATA"),
                [0_u32.to_le_bytes(), 1_f32.to_le_bytes()].concat(),
            )
            .sub(T(b"DNAM"), 100_u32.to_le_bytes())
            .sub(
                T(b"BOD2"),
                [1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
            )
            .sub(T(b"EITM"), REPLACEMENT_ENCHANTMENT.to_le_bytes()),
    );
    let mut game = game(fixture);
    let shared = game.shared_world();
    for item in [ARMOR, REPLACEMENT_ARMOR] {
        shared
            .borrow_mut()
            .equip_item_with_effects(id(ACTOR), id(item), &mut |name, class| {
                game.vm.create_object_of(name, class)
            })
            .unwrap();
    }

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    let world = game.world();
    let active = world.active_effects().next().unwrap();
    assert_eq!(active.spell, id(REPLACEMENT_ENCHANTMENT));
    assert_eq!(active.source, id(REPLACEMENT_ARMOR));
}

#[test]
fn mag_10_dispel_all_spells_keeps_worn_enchantment_active() {
    let mut game = game(enchanted_armor_fixture());
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    shared
        .borrow_mut()
        .dispel_all_spells(id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        });

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 110.0);
    assert_eq!(game.world().active_effects().count(), 1);
}

#[test]
fn mag_10_false_enchantment_condition_skips_its_effect() {
    let mut fixture = enchanted_armor_fixture();
    let enchantment = fixture
        .records
        .iter_mut()
        .find(|record| record.form_id == ENCHANTMENT)
        .unwrap();
    enchantment.subrecords.push(soe_esm::Subrecord::new(
        T(b"CTDA"),
        condition(14, 24, 200.0, 0),
    ));
    let mut game = game(fixture);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .equip_item_with_effects(id(ACTOR), id(ARMOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
