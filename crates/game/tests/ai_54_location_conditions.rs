// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-54 location and shared blocking condition fixtures.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR_BASE: u32 = 0x800;
const NPC_BASE: u32 = 0x801;
const OTHER_BASE: u32 = 0x802;
const CELL_A: u32 = 0x810;
const CELL_B: u32 = 0x811;
const REGION: u32 = 0x820;
const LOCATION: u32 = 0x830;
const OTHER_LOCATION: u32 = 0x831;
const KEYWORD: u32 = 0x840;
const QUEST: u32 = 0x850;
const ACTOR: u32 = 0x900;
const SAME_CELL_REF: u32 = 0x901;
const OTHER_CELL_REF: u32 = 0x902;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn cell(object: u32, owner: u32, location: u32, regions: &[u32]) -> Record {
    let mut record = Record::new(T(b"CELL"), object)
        .sub(T(b"DATA"), [1, 0])
        .sub(T(b"XOWN"), owner.to_le_bytes())
        .sub(T(b"XLCN"), location.to_le_bytes());
    if !regions.is_empty() {
        let bytes: Vec<u8> = regions
            .iter()
            .flat_map(|region| region.to_le_bytes())
            .collect();
        record = record.sub(T(b"XCLR"), bytes);
    }
    record
}

fn reference(object: u32, base: u32, cell: u32) -> Record {
    let mut record = Record::new(T(b"ACHR"), object).sub(T(b"NAME"), base.to_le_bytes());
    record.placement.cell = Some(cell);
    record
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE));
    plugin.push_top(Record::new(T(b"NPC_"), NPC_BASE));
    plugin.push_top(Record::new(T(b"NPC_"), OTHER_BASE));
    plugin.push_top(Record::new(T(b"STAT"), 0x860));
    plugin.push_top(Record::new(T(b"REGN"), REGION));
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(T(b"LCTN"), LOCATION).sub(T(b"KWDA"), KEYWORD.to_le_bytes()));
    plugin.push_top(Record::new(T(b"LCTN"), OTHER_LOCATION));
    plugin.push_top(cell(CELL_A, ACTOR_BASE, LOCATION, &[REGION]));
    plugin.push_top(cell(CELL_B, OTHER_BASE, OTHER_LOCATION, &[]));
    plugin.push_top(reference(0x14, NPC_BASE, CELL_A));
    plugin.push_top(reference(ACTOR, ACTOR_BASE, CELL_A));
    plugin.push_top(reference(SAME_CELL_REF, OTHER_BASE, CELL_A));
    plugin.push_top(reference(OTHER_CELL_REF, OTHER_BASE, CELL_B));
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"NEXT"), [])
            .sub(T(b"ALST"), 3_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ai54.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().actor(id(ACTOR));
    game
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

fn evaluate(
    game: &Game,
    subject: u32,
    c: &Condition,
    quest: Option<u32>,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(ACTOR_BASE),
            subject: id(subject),
            target: None,
            quest: quest.map(id),
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        c,
    )
}

#[test]
fn ai_54_blocking_and_lock_conditions_read_shared_runtime_state() {
    let game = fixture();
    game.world_mut().set_blocking(id(ACTOR), true).unwrap();
    assert_eq!(
        evaluate(&game, ACTOR, &condition(0, 0, 0, 1.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(0, 0, 0, 0.0), None),
        Ok(false)
    );
    game.world_mut().set_blocking(id(ACTOR), false).unwrap();
    assert_eq!(
        evaluate(&game, ACTOR, &condition(0, 0, 0, 0.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, 0x860, &condition(0, 0, 0, 0.0), None),
        Err(conditions::Error::Form(id(0x860)))
    );
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(0, 1, 0, 0.0), None),
        Err(conditions::Error::Parameter(0, 1))
    ));

    game.world_mut().reference(id(ACTOR)).unwrap().locked = true;
    assert_eq!(
        evaluate(&game, ACTOR, &condition(5, 0, 0, 1.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(5, 0, 0, 0.0), None),
        Ok(false)
    );
    game.world_mut().reference(id(ACTOR)).unwrap().locked = false;
    assert_eq!(
        evaluate(&game, ACTOR, &condition(5, 0, 0, 0.0), None),
        Ok(true)
    );
    assert!(matches!(
        evaluate(&game, 0x860, &condition(5, 0, 0, 0.0), None),
        Err(conditions::Error::Form(_))
    ));

    game.world_mut().reference(id(ACTOR)).unwrap().lock_level = 51;
    assert_eq!(
        evaluate(&game, ACTOR, &condition(65, 0, 0, 51.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(65, 0, 0, 52.0), None),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, 0x860, &condition(65, 0, 0, 0.0), None),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_54_cell_region_and_owned_cell_conditions_validate_context() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, ACTOR, &condition(32, SAME_CELL_REF, 0, 1.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(32, OTHER_CELL_REF, 0, 0.0), None),
        Ok(true)
    );
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(32, 0xaff, 0, 0.0), None),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(32, SAME_CELL_REF, 1, 1.0), None),
        Err(conditions::Error::Parameter(32, 1))
    ));

    assert_eq!(
        evaluate(&game, ACTOR, &condition(117, REGION, 0, 1.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(117, REGION, 0, 0.0), None),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(117, LOCATION, 0, 0.0), None),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(117, REGION, 1, 1.0), None),
        Err(conditions::Error::Parameter(117, 1))
    ));
    game.world_mut().reference(id(0x14)).unwrap().cell = Some(id(CELL_B));
    assert_eq!(
        evaluate(&game, ACTOR, &condition(117, REGION, 0, 0.0), None),
        Ok(true)
    );

    assert_eq!(
        evaluate(&game, ACTOR, &condition(146, 0, 0, 1.0), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(146, 0, 0, 0.0), None),
        Ok(false)
    );
    game.world_mut().reference(id(ACTOR)).unwrap().cell = None;
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(146, 0, 0, 0.0), None),
        Err(conditions::Error::Target(0))
    ));
}

#[test]
fn ai_54_editor_location_conditions_match_keyword_and_alias_contracts() {
    let game = fixture();
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(180, SAME_CELL_REF, KEYWORD, 1.0),
            None
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(180, OTHER_CELL_REF, KEYWORD, 0.0),
            None
        ),
        Ok(true)
    );
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(180, 0xaff, KEYWORD, 0.0), None),
        Err(conditions::Error::Form(_))
    ));

    assert!(
        game.world_mut()
            .force_alias(id(QUEST), 3, Some(id(SAME_CELL_REF)))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(181, 3, KEYWORD, 1.0), Some(QUEST)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(181, 3, KEYWORD, 0.0), Some(QUEST)),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(181, 3, KEYWORD, 0.0), None),
        Err(conditions::Error::Target(5))
    ));
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(181, 4, KEYWORD, 0.0), Some(QUEST)),
        Err(conditions::Error::Target(5))
    ));
    assert!(matches!(
        evaluate(&game, ACTOR, &condition(181, 3, 0xaff, 0.0), Some(QUEST)),
        Err(conditions::Error::Form(_))
    ));
}
