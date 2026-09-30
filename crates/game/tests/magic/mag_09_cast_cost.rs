use super::*;

const PERK: u32 = 0x700;

#[derive(Clone, Copy)]
struct Fixture {
    base_cost: f32,
    casting_time: f32,
    magnitude: f32,
    duration: u32,
    flags: u32,
    authored_cost: u32,
    charge_time: f32,
    perk: u32,
    cast_type: u32,
}

struct ExtraEffect {
    id: u32,
    base_cost: f32,
    casting_time: f32,
    magnitude: f32,
    duration: u32,
}

fn set_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_f32(data: &mut [u8], at: usize, value: f32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn records(fixture: Fixture) -> Plugin {
    records_with_extra_effect(fixture, None)
}

fn records_with_extra_effect(fixture: Fixture, extra: Option<ExtraEffect>) -> Plugin {
    let mut magic_effect = effect(1 << 1, 0, 24, false);
    let data = magic_effect
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .expect("effect DATA fixture");
    set_f32(&mut data.data, 4, fixture.base_cost);
    set_f32(&mut data.data, 48, fixture.casting_time);
    let mut spell = spell(SPELL, 0, false, fixture.magnitude, fixture.duration);
    let spit = spell
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .expect("spell SPIT fixture");
    set_u32(&mut spit.data, 0, fixture.authored_cost);
    set_u32(&mut spit.data, 4, fixture.flags);
    set_f32(&mut spit.data, 12, fixture.charge_time);
    set_u32(&mut spit.data, 16, fixture.cast_type);
    set_u32(&mut spit.data, 20, 3);
    set_u32(&mut spit.data, 32, fixture.perk);
    if let Some(extra) = extra {
        let mut efit = extra.magnitude.to_le_bytes().to_vec();
        efit.extend_from_slice(&0_u32.to_le_bytes());
        efit.extend_from_slice(&extra.duration.to_le_bytes());
        spell
            .subrecords
            .push(soe_esm::Subrecord::new(T(b"EFID"), extra.id.to_le_bytes()));
        spell
            .subrecords
            .push(soe_esm::Subrecord::new(T(b"EFIT"), efit));
        let mut data = vec![0; 152];
        set_u32(&mut data, 0, 1 << 1);
        set_f32(&mut data, 4, extra.base_cost);
        set_f32(&mut data, 48, extra.casting_time);
        let mut plugin = plugin(magic_effect, vec![spell]);
        plugin.push_top(Record::new(T(b"MGEF"), extra.id).sub(T(b"DATA"), data));
        plugin
    } else {
        plugin(magic_effect, vec![spell])
    }
}

#[test]
fn mag_09_manual_cost_and_charge_time_use_authored_values() {
    let game = game(records(Fixture {
        base_cost: 100.0,
        casting_time: 1.5,
        magnitude: 2.0,
        duration: 5,
        flags: 1,
        authored_cost: 37,
        charge_time: 2.25,
        perk: 0,
        cast_type: 1,
    }));

    assert_eq!(game.world().spell_charge_time(id(SPELL)).unwrap(), 2.25);
    assert_eq!(
        game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap(),
        37.0
    );
}

#[test]
fn mag_09_auto_cost_uses_effect_cost_magnitude_duration_and_max_cast_time() {
    let game = game(records_with_extra_effect(
        Fixture {
            base_cost: 10.0,
            casting_time: 1.0,
            magnitude: 2.0,
            duration: 5,
            flags: 0,
            authored_cost: 999,
            charge_time: 0.25,
            perk: 0,
            cast_type: 1,
        },
        Some(ExtraEffect {
            id: 0x201,
            base_cost: 10.0,
            casting_time: 1.75,
            magnitude: 2.0,
            duration: 5,
        }),
    ));

    assert_eq!(game.world().spell_charge_time(id(SPELL)).unwrap(), 1.75);
    assert!((game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap() - 20.0).abs() < 0.001);
}

#[test]
fn mag_09_auto_cost_floors_magnitude_and_zero_duration() {
    let game = game(records(Fixture {
        base_cost: 12.0,
        casting_time: 0.5,
        magnitude: 0.25,
        duration: 0,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: 0,
        cast_type: 1,
    }));

    assert!((game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap() - 12.0).abs() < 0.001);
}

#[test]
fn mag_09_concentration_cost_treats_effect_duration_as_ten_seconds() {
    let game = game(records(Fixture {
        base_cost: 12.0,
        casting_time: 0.5,
        magnitude: 2.0,
        duration: 1,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: 0,
        cast_type: 2,
    }));

    let expected = 12.0 * 2.0_f32.powf(1.1);
    assert!((game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap() - expected).abs() < 0.001);
}

#[test]
fn mag_09_half_cost_perk_modifies_expenditure() {
    let mut records = records(Fixture {
        base_cost: 10.0,
        casting_time: 1.0,
        magnitude: 2.0,
        duration: 5,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: PERK,
        cast_type: 1,
    });
    records.push_top(Record::new(T(b"PERK"), PERK));
    let game = game(records);
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .perks
        .insert(id(PERK));

    assert!((game.world_mut().spell_cost(id(SPELL), id(ACTOR)).unwrap() - 5.0).abs() < 0.001);
}

#[test]
fn mag_09_charged_cast_debits_magicka_once_and_applies_effect() {
    let mut game = game(records(Fixture {
        base_cost: 10.0,
        casting_time: 1.0,
        magnitude: 2.0,
        duration: 5,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: 0,
        cast_type: 1,
    }));
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(OTHER)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert_eq!(applied.len(), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 90.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 102.0);
    assert_eq!(
        game.world()
            .pending
            .iter()
            .filter(
                |pending| matches!(pending, Pending::Event { name, .. } if name == "OnSpellCast")
            )
            .count(),
        1
    );
}

#[test]
fn mag_09_insufficient_magicka_rejects_before_effect_or_resource_change() {
    let mut game = game(records(Fixture {
        base_cost: 10.0,
        casting_time: 1.0,
        magnitude: 2.0,
        duration: 5,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: 0,
        cast_type: 1,
    }));
    game.world_mut()
        .set_actor_value(id(ACTOR), "magicka", 9.0)
        .unwrap();
    let shared = game.shared_world();
    let error = shared
        .borrow_mut()
        .cast_charged_spell(
            id(SPELL),
            id(ACTOR),
            id(ACTOR),
            Some(id(OTHER)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap_err();

    assert!(matches!(error, MagicError::InsufficientMagicka));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 9.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_09_scripted_cast_does_not_emit_on_spell_cast() {
    let mut game = game(records(Fixture {
        base_cost: 10.0,
        casting_time: 1.0,
        magnitude: 2.0,
        duration: 5,
        flags: 0,
        authored_cost: 0,
        charge_time: 0.0,
        perk: 0,
        cast_type: 1,
    }));
    cast(&mut game, SPELL, OTHER).unwrap();

    assert!(!game
        .world()
        .pending
        .iter()
        .any(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnSpellCast")));
}
