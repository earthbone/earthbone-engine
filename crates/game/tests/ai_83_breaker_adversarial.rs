#![allow(clippy::unwrap_used)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn form(object: u32) -> Vec<u8> {
    object.to_le_bytes().to_vec()
}

fn condition(function: u16, first: u32, second: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [first, second],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (object, unique) in [(0x800, true), (0x801, false)] {
        let mut acbs = vec![0; 24];
        if unique {
            acbs[0..4].copy_from_slice(&0x20_u32.to_le_bytes());
        }
        plugin.push_top(Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"ACBS"), acbs));
    }
    for (object, base) in [(0x900, 0x800), (0x901, 0x801)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), form(base)));
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x920).sub(Tag(*b"NAME"), form(0x820)));
    let mut order = LoadOrder::new();
    order.add("Ai83Conditions.esm", plugin).unwrap();
    Game::new(order)
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_83_is_unique_reads_the_subjects_actor_base_flag() {
    let game = fixture();
    assert!(conditions::supports_function(641));
    for (subject, expected) in [(0x900, 1.0), (0x901, 0.0)] {
        assert_eq!(
            evaluate(&game, id(subject), &condition(641, 0, 0, expected)),
            Ok(true),
            "subject {subject:#x} should match IsUnique={expected}"
        );
        assert_eq!(
            evaluate(&game, id(subject), &condition(641, 0, 0, 1.0 - expected)),
            Ok(false),
            "subject {subject:#x} should not match the opposite IsUnique value"
        );
    }
}

#[test]
fn ai_83_is_unique_rejects_non_actor_context_and_unused_parameters() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x920), &condition(641, 0, 0, 0.0)),
        Err(conditions::Error::Form(id(0x920)))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(641, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(641, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(641, 0, 2, 0.0)),
        Err(conditions::Error::Parameter(641, 2))
    );
}

#[test]
fn ai_83_undocumented_interaction_and_undead_functions_report_diagnostics() {
    let game = fixture();
    for function in [703, 715] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn unique_condition_is_repeatable_for_both_base_flags() {
    let game = fixture();
    for _ in 0..3 {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(641, 0, 0, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x901), &condition(641, 0, 0, 0.0)),
            Ok(true)
        );
    }
}
