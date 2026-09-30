// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-01 actor queries through both CTDA evaluators and the existing death lifecycle.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{ai, conditions, package, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
    let game = Game::new(order);
    for name in ["health", "stamina"] {
        let mut world = game.world_mut();
        world.set_actor_value(id(0x900), name, 100.0).unwrap();
        world.mod_actor_value(id(0x900), name, 20.0).unwrap();
        world
            .mod_temporary_actor_value(id(0x900), name, 40.0)
            .unwrap();
        world.damage_actor_value(id(0x900), name, 80.0).unwrap();
    }
    game
}

fn condition(function: u16, parameter: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [parameter, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        c,
    )
}

fn package_evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> bool {
    let Comparison::Number(expected) = c.comparison else {
        panic!("fixture requires a numeric comparison");
    };
    ai::conditions_hold(
        &mut game.world_mut(),
        subject,
        id(0x800),
        None,
        &[package::Condition {
            comparison: c.operator(),
            flags: 0,
            value: package::CompValue::Float(expected),
            function: c.function,
            param1: c.parameters[0],
            param2: 0,
            run_on: c.run_on,
            reference: c.reference,
            param3: -1,
        }],
    )
}

#[test]
fn ai_01_queries_distinguish_components_in_both_evaluators() {
    let game = fixture();
    for (function, parameter, expected) in [
        (14, 24, 80.0),
        (128, 0, 0.5),
        (277, 24, 100.0),
        (430, 0, 0.5),
        (494, 24, 120.0),
    ] {
        let c = condition(function, parameter, expected);
        assert!(conditions::supports_function(function));
        assert_eq!(evaluate(&game, id(0x900), &c), Ok(true), "{function}");
        assert!(package_evaluate(&game, id(0x900), &c), "{function}");
        let wrong = condition(function, parameter, expected + 1.0);
        assert_eq!(evaluate(&game, id(0x900), &wrong), Ok(false), "{function}");
        assert!(!package_evaluate(&game, id(0x900), &wrong), "{function}");
    }
}

#[test]
fn ai_01_queries_reject_missing_nonactor_and_deleted_subjects() {
    let game = fixture();
    game.world_mut().reference(id(0x901)).unwrap().deleted = true;
    for function in [14, 128, 277, 408, 430, 494] {
        let parameter = if function == 408 { 0x900 } else { 24 };
        let c = condition(function, parameter, 0.0);
        for subject in [id(0xfff), id(0x910), id(0x901)] {
            assert_eq!(
                evaluate(&game, subject, &c),
                Err(conditions::Error::Form(subject))
            );
            assert!(!package_evaluate(&game, subject, &c));
        }
    }
}

#[test]
fn ai_01_actor_value_indices_cover_high_values_and_reject_sentinels() {
    let game = fixture();
    for (index, name) in [
        (8, "marksman"),
        (17, "speechcraft"),
        (33, "critchance"),
        (154, "attackdamagemult"),
        (163, "reflectdamage"),
    ] {
        game.world_mut()
            .set_actor_value(id(0x900), name, 37.0)
            .unwrap();
        for function in [14, 277, 494] {
            let c = condition(function, index, 37.0);
            assert_eq!(evaluate(&game, id(0x900), &c), Ok(true));
            assert!(package_evaluate(&game, id(0x900), &c));
        }
    }
    for function in [14, 277, 494] {
        for parameter in [164, u32::MAX] {
            let c = condition(function, parameter, 0.0);
            assert_eq!(
                evaluate(&game, id(0x900), &c),
                Err(conditions::Error::ActorValue(parameter))
            );
            assert!(!package_evaluate(&game, id(0x900), &c));
        }
    }
}

#[test]
fn ai_01_killer_query_uses_death_attribution_and_clears_after_resurrection() {
    let game = fixture();
    let killer = condition(408, 0x901, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &killer), Ok(false));
    assert!(!package_evaluate(&game, id(0x900), &killer));
    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    assert_eq!(evaluate(&game, id(0x900), &killer), Ok(true));
    assert!(package_evaluate(&game, id(0x900), &killer));
    let wrong = condition(408, 0x14, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &wrong), Ok(false));
    assert!(!package_evaluate(&game, id(0x900), &wrong));
    for parameter in [0, 0xfff, 0x910] {
        let invalid = condition(408, parameter, 1.0);
        assert!(matches!(
            evaluate(&game, id(0x900), &invalid),
            Err(conditions::Error::Form(_))
        ));
        assert!(!package_evaluate(&game, id(0x900), &invalid));
    }
    game.world_mut().resurrect_actor(id(0x900)).unwrap();
    assert_eq!(evaluate(&game, id(0x900), &killer), Ok(false));
    assert!(!package_evaluate(&game, id(0x900), &killer));
}

#[test]
fn ai_01_percentages_handle_zero_maximum_and_exhaustion() {
    let game = fixture();
    for (function, name) in [(128, "stamina"), (430, "health")] {
        game.world_mut()
            .damage_actor_value(id(0x900), name, 1000.0)
            .unwrap();
        let exhausted = condition(function, 0, 0.0);
        assert_eq!(evaluate(&game, id(0x900), &exhausted), Ok(true));
        assert!(package_evaluate(&game, id(0x900), &exhausted));
        game.world_mut()
            .set_actor_value(id(0x900), name, -60.0)
            .unwrap();
        let zero = condition(function, 0, 1.0);
        assert_eq!(evaluate(&game, id(0x900), &zero), Ok(true));
        assert!(package_evaluate(&game, id(0x900), &zero));
    }
}
