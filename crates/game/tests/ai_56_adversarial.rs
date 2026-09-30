// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-56 location and reference condition functions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const CELL: u32 = 0x802;
const WORLD: u32 = 0x803;
const ZONE: u32 = 0x804;
const LOCATION: u32 = 0x805;
const KEYWORD: u32 = 0x806;
const REF_TYPE: u32 = 0x807;
const OTHER_LOCATION: u32 = 0x808;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"WRLD"), WORLD));
    plugin.push_top(Record::new(Tag(*b"ECZN"), ZONE).sub(Tag(*b"DATA"), [0_u8; 12]));
    plugin.push_top(
        Record::new(Tag(*b"CELL"), CELL)
            .sub(Tag(*b"XEZN"), ZONE.to_le_bytes())
            .with_world(WORLD),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"XLCN"), LOCATION.to_le_bytes())
            .sub(Tag(*b"XLRT"), REF_TYPE.to_le_bytes())
            .with_cell(CELL),
    );
    plugin.push_top(
        Record::new(Tag(*b"LCTN"), LOCATION)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes())
            .sub(
                Tag(*b"ACSR"),
                [REF_TYPE.to_le_bytes(), ACTOR.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"LCTN"), OTHER_LOCATION));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"LCRT"), REF_TYPE));
    let mut order = LoadOrder::new();
    order.add("Ai56.esm", plugin).unwrap();
    Game::new(order)
}

trait Placement {
    fn with_world(self, world: u32) -> Self;
    fn with_cell(self, cell: u32) -> Self;
}

impl Placement for Record {
    fn with_world(mut self, world: u32) -> Self {
        self.placement.world = Some(world);
        self
    }

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
            quest: None,
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
        },
        condition,
    )
}

#[test]
fn ai_56_zone_and_map_marker_conditions_match_reference_state() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(445, ZONE, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(445, ZONE, 0, 0.0)), Ok(false));
    assert_eq!(
        evaluate(&game, &condition(445, LOCATION, 0, 1.0)),
        Err(conditions::Error::Form(id(LOCATION)))
    );
    assert_eq!(evaluate(&game, &condition(491, 0, 0, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(491, 0, 0, 1.0)), Ok(false));
    game.world_mut().reference(id(ACTOR)).unwrap().on_map = true;
    assert_eq!(evaluate(&game, &condition(491, 0, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(491, 1, 0, 1.0)),
        Err(conditions::Error::Parameter(491, 1))
    );
}

#[test]
fn ai_56_location_keyword_ref_type_and_editor_location_conditions_match_records() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(561, REF_TYPE, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(561, REF_TYPE, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, &condition(561, KEYWORD, 0, 1.0)),
        Err(conditions::Error::Form(id(KEYWORD)))
    );
    assert_eq!(evaluate(&game, &condition(562, KEYWORD, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(562, KEYWORD, 0, 0.0)), Ok(false));
    assert_eq!(
        evaluate(&game, &condition(562, REF_TYPE, 0, 1.0)),
        Err(conditions::Error::Form(id(REF_TYPE)))
    );
    assert_eq!(evaluate(&game, &condition(563, REF_TYPE, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(563, REF_TYPE, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(evaluate(&game, &condition(565, LOCATION, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(565, OTHER_LOCATION, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(565, OTHER_LOCATION, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, &condition(565, REF_TYPE, 0, 1.0)),
        Err(conditions::Error::Form(id(REF_TYPE)))
    );
    game.world_mut().reference(id(ACTOR)).unwrap().location = Some(id(OTHER_LOCATION));
    assert_eq!(evaluate(&game, &condition(562, KEYWORD, 0, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(565, LOCATION, 0, 1.0)), Ok(true));
    game.world_mut().reference(id(ACTOR)).unwrap().location = None;
    assert_eq!(
        evaluate(&game, &condition(562, KEYWORD, 0, 0.0)),
        Err(conditions::Error::LocationContext(id(ACTOR)))
    );
    assert_eq!(
        evaluate(&game, &condition(563, REF_TYPE, 0, 0.0)),
        Err(conditions::Error::LocationContext(id(ACTOR)))
    );
}

#[test]
fn ai_56_velocity_and_broken_lock_conditions_report_unmodeled_state() {
    let game = fixture();
    for expected in [0.0, 1.0] {
        assert_eq!(
            evaluate(&game, &condition(446, 0, 0, expected)),
            Err(conditions::Error::Unavailable(446))
        );
        assert_eq!(
            evaluate(&game, &condition(519, 0, 0, expected)),
            Err(conditions::Error::Unavailable(519))
        );
    }
    assert_eq!(
        evaluate(&game, &condition(446, 3, 0, 1.0)),
        Err(conditions::Error::Parameter(446, 3))
    );
    assert_eq!(
        evaluate(&game, &condition(446, 0, 1, 1.0)),
        Err(conditions::Error::Parameter(446, 1))
    );
    assert_eq!(
        evaluate(&game, &condition(519, 0, 1, 1.0)),
        Err(conditions::Error::Parameter(519, 1))
    );
    assert!(conditions::supports_function(446));
    assert!(conditions::supports_function(519));
}
