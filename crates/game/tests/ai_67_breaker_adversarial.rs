// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

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
    order.add("Ai67Breaker.esm", plugin).unwrap();
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

#[test]
fn ai_67_graph_integer_missing_name_and_missing_actor_are_errors() {
    let game = fixture();
    let mut query = condition(675, -7.0);
    query.strings[0] = Some("NoSuchGraphVariable".into());
    assert_eq!(
        evaluate(&game, id(0x900), &query),
        Err(conditions::Error::Variable("NoSuchGraphVariable".into()))
    );
    query.strings[0] = Some("IsStaggering".into());
    assert_eq!(
        evaluate(&game, id(0x910), &query),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_pathing_speed_is_zero_after_arrival_even_with_stale_waypoints() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.path.push([3.0, 0.0, 0.0]);
        actor.ai.arrived = true;
        actor.ai.speed = 42.0;
    }
    assert_eq!(evaluate(&game, id(0x900), &condition(683, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(704, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(704, 1.0)), Ok(false));
}

#[test]
fn ai_67_being_ridden_round_trip_preserves_both_boolean_values() {
    let game = fixture();
    for value in [true, false] {
        game.world_mut().actor(id(0x900)).unwrap().being_ridden = value;
        let bytes = game.world_mut().save_state();
        game.world_mut().actor(id(0x900)).unwrap().being_ridden = !value;
        game.world_mut().load_state(&bytes).unwrap();
        assert_eq!(
            evaluate(&game, id(0x900), &condition(714, f32::from(value))),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition(714, f32::from(!value))),
            Ok(false)
        );
    }
}

#[test]
fn ai_67_unavailable_conditions_diagnose_before_context_lookup() {
    let game = fixture();
    for function in [644, 684, 730] {
        assert_eq!(
            evaluate(&game, id(0x910), &condition(function, 1.0)),
            Err(conditions::Error::Unavailable(function))
        );
    }
}
