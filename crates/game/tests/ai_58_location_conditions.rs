// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-58 location alias, keyword data, loaded location and door conditions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, quest, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const OTHER_ACTOR: u32 = 0x802;
const CELL: u32 = 0x803;
const LOCATION: u32 = 0x804;
const OTHER_LOCATION: u32 = 0x805;
const KEYWORD: u32 = 0x806;
const OTHER_KEYWORD: u32 = 0x807;
const QUEST: u32 = 0x808;
const DOOR_BASE: u32 = 0x809;
const DOOR: u32 = 0x80a;
const UNLOCATED: u32 = 0x80b;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"KYWD"), OTHER_KEYWORD));
    plugin.push_top(Record::new(Tag(*b"LCTN"), LOCATION).sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"LCTN"), OTHER_LOCATION));
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL).sub(Tag(*b"XLCN"), LOCATION.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), OTHER_ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(Record::new(Tag(*b"DOOR"), DOOR_BASE));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), DOOR)
            .sub(Tag(*b"NAME"), DOOR_BASE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), UNLOCATED).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    let quest_record = quest::build_record(QUEST, "Ai58Quest", 0)
        .sub(Tag(*b"ALST"), 1_u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Reference")
        .sub(Tag(*b"FNAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"ALED"), Vec::new())
        .sub(Tag(*b"ALLS"), 2_u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Location")
        .sub(Tag(*b"ALED"), Vec::new());
    plugin.push_top(quest_record);
    let mut order = LoadOrder::new();
    order.add("Ai58.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.force_alias(id(QUEST), 1, Some(id(OTHER_ACTOR)));
        world.force_alias(id(QUEST), 2, Some(id(LOCATION)));
        world
            .location(id(LOCATION))
            .unwrap()
            .keyword_data
            .insert(id(KEYWORD), 2.5);
    }
    game
}

trait WithCell {
    fn with_cell(self, cell: u32) -> Self;
}

impl WithCell for Record {
    fn with_cell(mut self, cell: u32) -> Self {
        self.placement.cell = Some(cell);
        self
    }
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

fn evaluate(game: &Game, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(BASE),
            subject: id(ACTOR),
            target: None,
            quest: Some(id(QUEST)),
            tempering_enchanted: None,
        },
        c,
    )
}

#[test]
fn ai_58_same_current_location_as_reference_alias_checks_keyword() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(604, 1, KEYWORD, 1.0)), Ok(true));
    game.world_mut()
        .reference(id(OTHER_ACTOR))
        .unwrap()
        .location = Some(id(OTHER_LOCATION));
    assert_eq!(evaluate(&game, &condition(604, 1, KEYWORD, 0.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(604, 1, OTHER_KEYWORD, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(604, 2, KEYWORD, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(604, 99, KEYWORD, 0.0)),
        Err(conditions::Error::Target(5))
    );
    let missing_location = conditions::Context {
        owner: id(BASE),
        subject: id(UNLOCATED),
        target: None,
        quest: Some(id(QUEST)),
        tempering_enchanted: None,
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            missing_location,
            &condition(604, 1, KEYWORD, 0.0),
        ),
        Err(conditions::Error::LocationContext(id(UNLOCATED)))
    );
}

#[test]
fn ai_58_location_alias_is_location_compares_location_forms() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(605, 2, LOCATION, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(605, 2, OTHER_LOCATION, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(605, 1, LOCATION, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(605, 99, LOCATION, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(605, 2, KEYWORD, 0.0)),
        Err(conditions::Error::Form(id(KEYWORD)))
    );
}

#[test]
fn ai_58_location_alias_has_keyword_checks_location_keywords() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(610, 2, KEYWORD, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(610, 2, OTHER_KEYWORD, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(610, 1, KEYWORD, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(610, 2, BASE, 0.0)),
        Err(conditions::Error::Form(id(BASE)))
    );
}

#[test]
fn ai_58_location_loaded_conditions_report_unmodeled_runtime_contract() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, &condition(625, LOCATION, 0, 1.0)),
        Err(conditions::Error::Unavailable(625))
    );
    assert_eq!(
        evaluate(&game, &condition(625, OTHER_LOCATION, 0, 0.0)),
        Err(conditions::Error::Unavailable(625))
    );
    assert_eq!(
        evaluate(&game, &condition(625, BASE, 0, 0.0)),
        Err(conditions::Error::Form(id(BASE)))
    );
    assert_eq!(
        evaluate(&game, &condition(626, 2, 0, 1.0)),
        Err(conditions::Error::Unavailable(626))
    );
    assert_eq!(
        evaluate(&game, &condition(626, 1, 0, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(626, 99, 0, 0.0)),
        Err(conditions::Error::Target(5))
    );
}

#[test]
fn ai_58_keyword_data_for_current_location_reads_location_state() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(651, KEYWORD, 0, 2.5)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(651, OTHER_KEYWORD, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(651, BASE, 0, 0.0)),
        Err(conditions::Error::Form(id(BASE)))
    );
    let missing_location = conditions::Context {
        owner: id(BASE),
        subject: id(UNLOCATED),
        target: None,
        quest: Some(id(QUEST)),
        tempering_enchanted: None,
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            missing_location,
            &condition(651, KEYWORD, 0, 0.0),
        ),
        Err(conditions::Error::LocationContext(id(UNLOCATED)))
    );
}

#[test]
fn ai_58_unlocked_door_condition_reads_live_lock_state() {
    let game = fixture();
    let unlocked_context = conditions::Context {
        owner: id(BASE),
        subject: id(DOOR),
        target: None,
        quest: Some(id(QUEST)),
        tempering_enchanted: None,
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            unlocked_context,
            &condition(718, 0, 0, 1.0),
        ),
        Ok(true)
    );
    game.world_mut().reference(id(DOOR)).unwrap().locked = true;
    let locked_context = conditions::Context {
        owner: id(BASE),
        subject: id(DOOR),
        target: None,
        quest: Some(id(QUEST)),
        tempering_enchanted: None,
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            locked_context,
            &condition(718, 0, 0, 0.0),
        ),
        Ok(true)
    );
    let invalid_context = conditions::Context {
        owner: id(BASE),
        subject: id(ACTOR),
        target: None,
        quest: Some(id(QUEST)),
        tempering_enchanted: None,
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            invalid_context,
            &condition(718, 0, 0, 0.0),
        ),
        Err(conditions::Error::Form(id(ACTOR)))
    );
    assert_eq!(
        evaluate(&game, &condition(718, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(718, 1))
    );
}

#[test]
fn ai_58_location_conditions_are_registered() {
    for function in [604, 605, 610, 625, 626, 651, 718] {
        assert!(conditions::supports_function(function));
    }
}
