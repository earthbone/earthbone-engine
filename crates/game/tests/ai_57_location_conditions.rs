// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-57 location alias, blocking and location reference count conditions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, quest, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const DEAD_ACTOR: u32 = 0x802;
const CELL: u32 = 0x803;
const LOCATION: u32 = 0x804;
const OTHER_LOCATION: u32 = 0x805;
const KEYWORD: u32 = 0x806;
const REF_TYPE: u32 = 0x807;
const QUEST: u32 = 0x808;
const UNLOCATED: u32 = 0x809;
const OTHER_REF_TYPE: u32 = 0x80a;
const REMOVED_ACTOR: u32 = 0x80b;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL).sub(Tag(*b"XLCN"), LOCATION.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"XLCN"), LOCATION.to_le_bytes())
            .sub(Tag(*b"XLRT"), REF_TYPE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), DEAD_ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"XLCN"), LOCATION.to_le_bytes())
            .sub(Tag(*b"XLRT"), REF_TYPE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), REMOVED_ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"XLCN"), LOCATION.to_le_bytes())
            .sub(Tag(*b"XLRT"), REF_TYPE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(Record::new(Tag(*b"REFR"), UNLOCATED).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    let special = [
        [
            REF_TYPE.to_le_bytes(),
            ACTOR.to_le_bytes(),
            CELL.to_le_bytes(),
            [0; 4],
        ]
        .concat(),
        [
            REF_TYPE.to_le_bytes(),
            DEAD_ACTOR.to_le_bytes(),
            CELL.to_le_bytes(),
            [0; 4],
        ]
        .concat(),
        [
            REF_TYPE.to_le_bytes(),
            REMOVED_ACTOR.to_le_bytes(),
            CELL.to_le_bytes(),
            [0; 4],
        ]
        .concat(),
    ]
    .concat();
    plugin.push_top(
        Record::new(Tag(*b"LCTN"), LOCATION)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes())
            .sub(Tag(*b"ACSR"), special)
            .sub(Tag(*b"RCSR"), REMOVED_ACTOR.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"LCTN"), OTHER_LOCATION));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"LCRT"), REF_TYPE));
    plugin.push_top(Record::new(Tag(*b"LCRT"), OTHER_REF_TYPE));
    plugin.push_top(
        quest::build_record(QUEST, "Ai57Quest", 0)
            .sub(Tag(*b"ALLS"), 1_u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "Location")
            .sub(Tag(*b"ALED"), Vec::new()),
    );
    let mut order = LoadOrder::new();
    order.add("Ai57.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.force_alias(id(QUEST), 1, Some(id(LOCATION)));
        world.actor(id(ACTOR)).unwrap().blocking = true;
        world.actor(id(DEAD_ACTOR)).unwrap().dead = true;
        world.actor(id(REMOVED_ACTOR)).unwrap().dead = true;
        world.location(id(LOCATION)).unwrap().cleared = true;
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

fn evaluate(game: &Game, condition: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(BASE),
            subject: id(ACTOR),
            target: None,
            quest: Some(id(QUEST)),
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

#[test]
fn ai_57_editor_location_alias_matches_subject_placement() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(567, 1, 0, 1.0)), Ok(true));
    game.world_mut()
        .force_alias(id(QUEST), 1, Some(id(OTHER_LOCATION)));
    assert_eq!(evaluate(&game, &condition(567, 1, 0, 0.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(567, 9, 0, 1.0)),
        Err(conditions::Error::Target(5))
    );
}

#[test]
fn ai_57_blocking_reads_live_actor_block_state() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(569, 0, 0, 1.0)), Ok(true));
    game.world_mut().actor(id(ACTOR)).unwrap().blocking = false;
    assert_eq!(evaluate(&game, &condition(569, 0, 0, 0.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(569, 1, 0, 1.0)),
        Err(conditions::Error::Parameter(569, 1))
    );
    let invalid_subject = conditions::Context {
        owner: id(BASE),
        subject: id(UNLOCATED),
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
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            invalid_subject,
            &condition(569, 0, 0, 0.0),
        ),
        Err(conditions::Error::Form(id(UNLOCATED)))
    );
}

#[test]
fn ai_57_location_ref_type_counts_split_dead_and_alive_entries() {
    let game = fixture();
    for function in [591, 592, 600, 601] {
        let alias_or_location = if function < 600 { LOCATION } else { 1 };
        assert_eq!(
            evaluate(
                &game,
                &condition(function, alias_or_location, REF_TYPE, 1.0)
            ),
            Ok(true),
            "function {function}"
        );
        assert_eq!(
            evaluate(
                &game,
                &condition(function, alias_or_location, OTHER_REF_TYPE, 0.0)
            ),
            Ok(true),
            "function {function}"
        );
        assert_eq!(
            evaluate(
                &game,
                &condition(function, alias_or_location, REF_TYPE, 2.0)
            ),
            Ok(false),
            "function {function}"
        );
    }
    assert_eq!(
        evaluate(&game, &condition(591, LOCATION, KEYWORD, 0.0)),
        Err(conditions::Error::Form(id(KEYWORD)))
    );
    assert_eq!(
        evaluate(&game, &condition(600, 9, REF_TYPE, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(592, LOCATION, KEYWORD, 0.0)),
        Err(conditions::Error::Form(id(KEYWORD)))
    );
    assert_eq!(
        evaluate(&game, &condition(601, 9, REF_TYPE, 0.0)),
        Err(conditions::Error::Target(5))
    );
}

#[test]
fn ai_57_location_alias_cleared_reads_location_state() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(598, 1, 0, 1.0)), Ok(true));
    game.world_mut().location(id(LOCATION)).unwrap().cleared = false;
    assert_eq!(evaluate(&game, &condition(598, 1, 0, 0.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(598, 9, 0, 0.0)),
        Err(conditions::Error::Target(5))
    );
}

#[test]
fn ai_57_same_current_location_checks_reference_and_keyword() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, &condition(603, DEAD_ACTOR, KEYWORD, 1.0)),
        Ok(true)
    );
    game.world_mut().reference(id(DEAD_ACTOR)).unwrap().location = Some(id(OTHER_LOCATION));
    assert_eq!(
        evaluate(&game, &condition(603, DEAD_ACTOR, KEYWORD, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(603, DEAD_ACTOR, REF_TYPE, 0.0)),
        Err(conditions::Error::Form(id(REF_TYPE)))
    );
    assert_eq!(
        evaluate(&game, &condition(603, UNLOCATED, KEYWORD, 0.0)),
        Err(conditions::Error::LocationContext(id(UNLOCATED)))
    );
}

#[test]
fn ai_57_location_conditions_are_registered() {
    for function in [567, 569, 591, 592, 598, 600, 601, 603] {
        assert!(conditions::supports_function(function));
    }
}
