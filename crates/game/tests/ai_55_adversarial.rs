// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-55 location, cell, owner and package CTDA functions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, package, quest, Game};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const OWNER: u32 = 0x802;
const CELL: u32 = 0x803;
const WORLD: u32 = 0x804;
const OTHER_REF: u32 = 0x805;
const LOCATION: u32 = 0x806;
const CHILD: u32 = 0x807;
const FORM_LIST: u32 = 0x808;
const QUEST: u32 = 0x809;
const PACKAGE: u32 = 0x80a;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(Record::new(Tag(*b"WRLD"), WORLD));
    let mut cell = Record::new(Tag(*b"CELL"), CELL)
        .sub(Tag(*b"DATA"), [1_u8, 0])
        .sub(Tag(*b"XOWN"), OWNER.to_le_bytes());
    cell.placement.world = Some(WORLD);
    plugin.push_top(cell);
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR)
            .sub(Tag(*b"NAME"), BASE.to_le_bytes())
            .sub(Tag(*b"XLCN"), CHILD.to_le_bytes())
            .with_placement(CELL, WORLD),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), OTHER_REF)
            .sub(Tag(*b"NAME"), OWNER.to_le_bytes())
            .with_placement(CELL, WORLD),
    );
    plugin.push_top(Record::new(Tag(*b"LCTN"), LOCATION));
    plugin.push_top(Record::new(Tag(*b"LCTN"), CHILD).sub(Tag(*b"PNAM"), LOCATION.to_le_bytes()));
    plugin
        .push_top(Record::new(Tag(*b"FLST"), FORM_LIST).sub(Tag(*b"LNAM"), LOCATION.to_le_bytes()));
    plugin.push_top(
        quest::build_record(QUEST, "Ai55Quest", 0)
            .sub(Tag(*b"ALLS"), 1_u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "CurrentLocation")
            .sub(Tag(*b"ALED"), Vec::new()),
    );
    let pack = package::build_record(PACKAGE, "Ai55Package", 0, package::Schedule::default());
    let pack = package::close_header(pack, None);
    let pack =
        package::with_location_input(pack, 0, package::Location::NearReference(OTHER_REF), 10);
    plugin.push_top(package::with_actions(package::finish(pack)));

    let mut order = LoadOrder::new();
    order.add("Ai55.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.force_alias(id(QUEST), 1, Some(id(LOCATION)));
        world.location(id(LOCATION)).unwrap().cleared = true;
        world.actor(id(ACTOR)).unwrap().ai.package = Some(id(PACKAGE));
    }
    game
}

trait WithPlacement {
    fn with_placement(self, cell: u32, world: u32) -> Self;
}

impl WithPlacement for Record {
    fn with_placement(mut self, cell: u32, world: u32) -> Self {
        self.placement.cell = Some(cell);
        self.placement.world = Some(world);
        self
    }
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
fn ai_55_cell_conditions_match_cell_membership_and_ownership() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, &condition(230, CELL, OTHER_REF, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(230, CELL, OTHER_REF, 0.0)),
        Ok(false)
    );
    assert_eq!(evaluate(&game, &condition(280, CELL, OWNER, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(280, CELL, OWNER + 1, 0.0)),
        Err(conditions::Error::Form(id(OWNER + 1)))
    );
    assert_eq!(
        evaluate(&game, &condition(230, CELL, 0x999, 1.0)),
        Err(conditions::Error::Form(id(0x999)))
    );
}

#[test]
fn ai_55_location_and_worldspace_conditions_follow_live_form_state() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(250, LOCATION, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(250, LOCATION, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(evaluate(&game, &condition(310, WORLD, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(310, CELL, 0, 0.0)),
        Err(conditions::Error::Form(id(CELL)))
    );
    assert_eq!(
        evaluate(&game, &condition(250, WORLD, 0, 0.0)),
        Err(conditions::Error::Form(id(WORLD)))
    );
}

#[test]
fn ai_55_alias_and_form_list_conditions_include_child_locations() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(360, 1, 0, 1.0)), Ok(true));
    assert_eq!(
        evaluate(&game, &condition(360, 2, 0, 0.0)),
        Err(conditions::Error::Target(5))
    );
    assert_eq!(
        evaluate(&game, &condition(444, FORM_LIST, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, &condition(444, FORM_LIST, 0, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, &condition(444, LOCATION, 0, 0.0)),
        Err(conditions::Error::Form(id(LOCATION)))
    );
}

#[test]
fn ai_55_package_location_and_hit_location_report_context_limits() {
    let game = fixture();
    assert_eq!(evaluate(&game, &condition(325, 0, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, &condition(325, 0, 0, 0.0)), Ok(false));
    assert_eq!(
        evaluate(&game, &condition(325, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(325, 1))
    );
    game.world_mut().actor(id(ACTOR)).unwrap().ai.package = None;
    assert_eq!(
        evaluate(&game, &condition(325, 0, 0, 0.0)),
        Err(conditions::Error::PackageContext(id(ACTOR)))
    );
    assert_eq!(
        evaluate(&game, &condition(390, 0, 0, 0.0)),
        Err(conditions::Error::Function(390))
    );
    assert!(!conditions::supports_function(390));
}

#[test]
fn ai_55_single_parameter_conditions_reject_unused_parameter_two() {
    let game = fixture();
    for (function, parameter) in [
        (250, LOCATION),
        (310, WORLD),
        (325, 0),
        (360, 1),
        (444, FORM_LIST),
    ] {
        assert_eq!(
            evaluate(&game, &condition(function, parameter, 1, 0.0)),
            Err(conditions::Error::Parameter(function, 1)),
            "function {function}"
        );
    }
}

#[test]
fn ai_55_in_cell_param_tests_the_explicit_reference_independently() {
    let game = fixture();
    game.world_mut().reference(id(ACTOR)).unwrap().cell = None;
    assert_eq!(
        evaluate(&game, &condition(230, CELL, OTHER_REF, 1.0)),
        Ok(true)
    );
}
