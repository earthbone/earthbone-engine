// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin,
    Record, Tag,
};
use soe_game::package::{self, condition, CompValue, Condition as PackageCondition, Location, Schedule};
use soe_game::{conditions, Game};

const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const DEST: u32 = 0x804;
const ENABLED: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::finish(package::close_header(
        package::build_template(TEMPLATE, "Hover"),
        None,
    ));
    let template = package::with_procedure(template, "Hover", &[0, 1], false);
    let template = package::with_input_name(template, 0, "Target", true);
    let template = package::with_input_name(template, 1, "Height", true);
    plugin.push_top(package::with_actions(template));

    let enabled = PackageCondition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: ENABLED,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let hover = package::with_location_input(
        package::with_float_input(
            package::close_header(
                package::with_condition(
                    package::build_record(PACK, "HoverPackage", 0, Schedule::default()),
                    &enabled,
                ),
                Some(TEMPLATE),
            ),
            1,
            40.0,
        ),
        0,
        Location::NearReference(DEST),
        0,
    );
    plugin.push_top(package::with_actions(package::finish(hover)));
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), BASE).sub(Tag(*b"PKID"), PACK.to_le_bytes()),
    );
    for (tag, form, base, position) in [
        (*b"ACHR", ACTOR, BASE, [0.0f32; 3]),
        (*b"REFR", DEST, 0, [50.0f32, 25.0, 10.0]),
    ] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(Tag(tag), form)
                .sub(Tag(*b"NAME"), base.to_le_bytes())
                .sub(Tag(*b"DATA"), data),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"GLOB"), ENABLED)
            .sub(Tag(*b"FNAM"), b"f")
            .sub(Tag(*b"FLTV"), 1.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ai65Flight.esm", Plugin::parse(&plugin.to_bytes()).unwrap()).unwrap();
    order
}

fn flying_condition(expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 594,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &mut Game, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(PACK),
            subject: id(ACTOR),
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
fn ai_65_get_is_flying_tracks_hover_start_repeated_reads_and_stop() {
    let mut game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    let flying = flying_condition(1.0);
    let grounded = flying_condition(0.0);

    assert_eq!(evaluate(&mut game, &flying), Ok(false));
    assert_eq!(evaluate(&mut game, &grounded), Ok(true));

    game.tick(0.1);
    assert_eq!(evaluate(&mut game, &flying), Ok(true));
    assert_eq!(evaluate(&mut game, &flying), Ok(true));

    game.world_mut().set_global(id(ENABLED), 0.0);
    game.world_mut().actor(id(ACTOR)).unwrap().ai.reevaluate = true;
    game.tick(0.0);
    assert_eq!(evaluate(&mut game, &flying), Ok(false));
    assert_eq!(evaluate(&mut game, &grounded), Ok(true));
}
