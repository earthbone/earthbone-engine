// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai65Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_65_flying_is_current_state_not_flight_permission() {
    let game = fixture();
    let flying = condition(594, 1.0);
    let grounded = condition(594, 0.0);
    assert_eq!(evaluate(&game, id(0x900), &grounded), Ok(true));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.allow_flying = true;
        assert!(!actor.flying);
    }
    assert_eq!(evaluate(&game, id(0x900), &grounded), Ok(true));
    game.world_mut().actor(id(0x900)).unwrap().flying = true;
    assert_eq!(evaluate(&game, id(0x900), &flying), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &grounded), Ok(false));
    assert_eq!(evaluate(&game, id(0x910), &flying), Err(conditions::Error::Form(id(0x910))));
    let mut bad_parameter = flying;
    bad_parameter.parameters[0] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &bad_parameter),
        Err(conditions::Error::Parameter(594, 1))
    );
}

#[test]
fn ai_65_flight_state_participates_in_world_state_hash() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap();
    let grounded_hash = game.world().state_hash();
    game.world_mut().actor(id(0x900)).unwrap().flying = true;
    assert_ne!(game.world().state_hash(), grounded_hash);
}

#[test]
fn ai_65_graph_float_rejects_nonfinite_runtime_values_and_extra_parameter() {
    let game = fixture();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_floats
        .insert("speedblend".into(), f32::NAN);
    let mut c = condition(447, 0.0);
    c.strings[0] = Some("SpeedBlend".into());
    assert_eq!(evaluate(&game, id(0x900), &c), Err(conditions::Error::NonFinite));
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_floats
        .insert("speedblend".into(), 1.0);
    c.parameters[1] = 1;
    assert_eq!(evaluate(&game, id(0x900), &c), Err(conditions::Error::Parameter(447, 1)));
}

#[test]
fn ai_65_uncontracted_functions_report_diagnostics_for_actor_and_reference() {
    let game = fixture();
    for function in [358, 450, 584, 589, 613] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0.0)),
            Err(conditions::Error::UnavailableContract(function))
        );
        assert_eq!(
            evaluate(&game, id(0x910), &condition(function, 0.0)),
            Err(conditions::Error::UnavailableContract(function))
        );
    }
}
