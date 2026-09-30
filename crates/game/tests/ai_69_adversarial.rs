// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-69 quest and reference condition queries.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};
use soe_papyrus::asm::{i, ScriptBuilder};
use soe_papyrus::Value;

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const OBJECT: u32 = 0x802;
const QUEST: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), OBJECT));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x804).sub(Tag(*b"NAME"), OBJECT.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"QUST"), QUEST));
    let mut order = LoadOrder::new();
    order.add("Ai69.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, p1: u32, p2: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [p1, p2],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, subject: u32, condition: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(BASE),
            subject: id(subject),
            target: None,
            quest: None,
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        condition,
    )
}

fn attach_conditional(game: &mut Game, form: u32) -> soe_papyrus::ObjectId {
    let mut script = ScriptBuilder::new("Ai69Variables", "Quest")
        .auto_property("Value", "Int", i(1))
        .build();
    script.user_flags.push(soe_papyrus::pex::UserFlag {
        name: "conditional".into(),
        bit: 3,
    });
    script.objects[0].user_flags = 8;
    script.objects[0].variables[0].user_flags = 8;
    game.vm.load(script);
    let object = game.object_of_form(id(form));
    game.vm.attach(object, "Ai69Variables").unwrap();
    object
}

#[test]
fn ai_69_script_and_quest_variables_read_conditional_values() {
    let mut game = game();
    let reference = attach_conditional(&mut game, ACTOR);
    let quest = attach_conditional(&mut game, QUEST);
    game.vm
        .set_var(reference, "Ai69Variables", "::Value_var", Value::Int(4))
        .unwrap();
    game.vm
        .set_var(quest, "Ai69Variables", "::Value_var", Value::Int(7))
        .unwrap();

    for (function, form, value) in [(53, ACTOR, 4.0), (79, QUEST, 7.0)] {
        let mut c = condition(function, form, 0, value);
        c.strings[1] = Some("::Value_var".into());
        assert!(conditions::supports_function(function));
        assert_eq!(evaluate(&game, ACTOR, &c), Ok(true));
        c.comparison = Comparison::Number(value + 1.0);
        assert_eq!(evaluate(&game, ACTOR, &c), Ok(false));
    }

    let mut missing = condition(53, ACTOR, 0, 0.0);
    missing.strings[1] = Some("absent".into());
    assert!(matches!(
        evaluate(&game, ACTOR, &missing),
        Err(conditions::Error::Variable(_))
    ));
    let mut invalid_reference = condition(53, 0x999, 0, 0.0);
    invalid_reference.strings[1] = Some("::Value_var".into());
    assert!(matches!(
        evaluate(&game, ACTOR, &invalid_reference),
        Err(conditions::Error::Form(_))
    ));
    let mut invalid_quest = condition(79, ACTOR, 0, 0.0);
    invalid_quest.strings[1] = Some("::Value_var".into());
    assert!(matches!(
        evaluate(&game, ACTOR, &invalid_quest),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, 0x804, &condition(79, QUEST, 0, 0.0)),
        Err(conditions::Error::Variable(_))
    ));
}

#[test]
fn ai_69_destruction_stage_uses_reference_state_and_save_round_trip() {
    let game = game();
    let mut c = condition(470, 0, 0, -1.0);
    assert_eq!(evaluate(&game, 0x804, &c), Ok(true));
    c.comparison = Comparison::Number(0.0);
    assert_eq!(evaluate(&game, 0x804, &c), Ok(false));
    assert!(matches!(
        evaluate(&game, BASE, &c),
        Err(conditions::Error::Form(_))
    ));

    game.world_mut()
        .reference(id(0x804))
        .unwrap()
        .destruction_stage = 2;
    let bytes = game.world_mut().save_state();
    game.world_mut()
        .reference(id(0x804))
        .unwrap()
        .destruction_stage = -1;
    game.world_mut().load_state(&bytes).unwrap();
    assert_eq!(
        game.world_mut()
            .reference(id(0x804))
            .unwrap()
            .destruction_stage,
        2
    );
    c.comparison = Comparison::Number(2.0);
    assert_eq!(evaluate(&game, 0x804, &c), Ok(true));
    assert!(matches!(
        evaluate(&game, 0xfff, &c),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_69_critical_stage_matches_ck_values_and_rejects_bad_context() {
    let game = game();
    game.world_mut().actor(id(ACTOR)).unwrap().critical_stage = 3;
    let c = condition(528, 3, 0, 1.0);
    assert_eq!(evaluate(&game, ACTOR, &c), Ok(true));
    assert_eq!(
        evaluate(&game, ACTOR, &condition(528, 2, 0, 1.0)),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, 0x804, &c),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(528, 5, 0, 1.0)),
        Err(conditions::Error::Parameter(528, 5))
    ));
}

#[test]
fn ai_69_critical_stage_survives_save_load() {
    let mut game = game();
    game.world_mut().actor(id(ACTOR)).unwrap().critical_stage = 3;
    let bytes = game.save_state().unwrap();
    game.world_mut().actor(id(ACTOR)).unwrap().critical_stage = 0;
    game.load_state(&bytes).unwrap();
    assert_eq!(evaluate(&game, ACTOR, &condition(528, 3, 0, 1.0)), Ok(true));
}

#[test]
fn ai_69_undocumented_event_and_landing_queries_return_diagnostics() {
    let game = game();
    for function in [576, 646, 647] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, 0, 1.0)),
            Err(conditions::Error::Function(function))
        );
    }
}
