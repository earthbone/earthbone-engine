// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-40 combat condition contracts and diagnostics.

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
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x910));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x920));
    let mut order = LoadOrder::new();
    order.add("Ai40.esm", plugin).unwrap();
    Game::new(order)
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

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    condition: &Condition,
) -> Result<bool, conditions::Error> {
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
            skill_use: None,
            light_level: None,
        },
        condition,
    )
}

fn package_evaluate(game: &Game, subject: GlobalFormId, condition: &Condition) -> bool {
    let Comparison::Number(expected) = condition.comparison else {
        panic!("fixture requires a numeric comparison");
    };
    ai::conditions_hold(
        &mut game.world_mut(),
        subject,
        id(0x800),
        None,
        &[package::Condition {
            comparison: condition.operator(),
            flags: 0,
            value: package::CompValue::Float(expected),
            function: condition.function,
            param1: condition.parameters[0],
            param2: condition.parameters[1],
            run_on: condition.run_on,
            reference: condition.reference,
            param3: condition.parameter_3,
        }],
    )
}

#[test]
fn ai_40_bleeding_out_reads_live_unconscious_state_and_excludes_death() {
    let game = fixture();
    for (bleeding_out, expected) in [(false, 0.0), (true, 1.0)] {
        game.world_mut().actor(id(0x900)).unwrap().unconscious = bleeding_out;
        let query = condition(580, 0, expected);
        assert!(conditions::supports_function(580));
        assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(580, 0, 1.0 - expected)),
            Ok(false)
        );
        assert!(package_evaluate(&game, id(0x900), &query));
        assert!(!package_evaluate(
            &game,
            id(0x900),
            &condition(580, 0, 1.0 - expected)
        ));
    }
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.unconscious = true;
        actor.dead = true;
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(580, 0, 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_40_no_bleedout_recovery_reads_flag_independent_of_current_bleedout() {
    let game = fixture();
    for (flag, expected) in [(false, 0.0), (true, 1.0)] {
        game.world_mut()
            .actor(id(0x900))
            .unwrap()
            .no_bleedout_recovery = flag;
        assert_eq!(
            evaluate(&game, id(0x900), &condition(694, 0, expected)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition(694, 0, 1.0 - expected)),
            Ok(false)
        );
        assert!(package_evaluate(
            &game,
            id(0x900),
            &condition(694, 0, expected)
        ));
        assert!(!package_evaluate(
            &game,
            id(0x900),
            &condition(694, 0, 1.0 - expected)
        ));
    }
}

#[test]
fn ai_40_staggered_condition_diagnoses_missing_contract_even_with_live_state() {
    let game = fixture();
    game.world_mut().clock.real_seconds = 5.0;
    game.world_mut().actor(id(0x900)).unwrap().stagger_end_time = Some(6.0);
    assert!(!conditions::supports_function(701));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(701, 0, 1.0)),
        Err(conditions::Error::UnavailableContractReason(
            701,
            "the local CK corpus does not define IsStaggered condition behavior"
        ))
    );
}

#[test]
fn ai_40_state_queries_reject_parameters_and_invalid_actor_contexts() {
    let game = fixture();
    for function in [580, 694, 701] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    game.world_mut().reference(id(0x901)).unwrap().deleted = true;
    for function in [580, 672, 673, 674, 678, 694, 697, 701] {
        for subject in [id(0xfff), id(0x910), id(0x901)] {
            let parameter = match function {
                678 => 0x901,
                697 => 0x920,
                _ => 0,
            };
            assert_eq!(
                evaluate(&game, subject, &condition(function, parameter, 0.0)),
                Err(conditions::Error::Form(subject)),
                "function {function}, subject {subject:?}"
            );
        }
    }
}

#[test]
fn ai_40_attack_state_without_a_runtime_contract_returns_explicit_diagnostics() {
    let game = fixture();
    let cases = [
        (672, 0, "the runtime has no live attack-state producer"),
        (
            673,
            0,
            "MeleeHit records confirmed contacts, not the active animation state",
        ),
        (
            674,
            0,
            "the runtime does not track the actor's last hostile identity",
        ),
        (
            678,
            0x901,
            "no damage forecast contract is documented for this condition",
        ),
        (
            697,
            0x920,
            "the runtime has no active attack type to compare with the keyword",
        ),
        (
            701,
            0,
            "the local CK corpus does not define IsStaggered condition behavior",
        ),
    ];
    for (function, parameter, reason) in cases {
        assert!(!conditions::supports_function(function));
        for expected in [0.0, 1.0] {
            let error = if function == 701 {
                conditions::Error::UnavailableContractReason(function, reason)
            } else {
                conditions::Error::UnavailableState { function, reason }
            };
            assert_eq!(
                evaluate(&game, id(0x900), &condition(function, parameter, expected)),
                Err(error),
                "function {function} must diagnose instead of inventing a boolean"
            );
        }
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(678, 0x800, 0.0)),
        Err(conditions::Error::Form(id(0x800)))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(697, 0x910, 0.0)),
        Err(conditions::Error::Form(id(0x910)))
    );
    for function in [672, 673, 674] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    for function in [678, 697] {
        let mut extra = condition(function, if function == 678 { 0x901 } else { 0x920 }, 0.0);
        extra.parameters[1] = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &extra),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
}
