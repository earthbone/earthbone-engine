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
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai67Breaker3.esm", plugin).unwrap();
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
fn ai_67_graph_integer_is_case_insensitive_and_reports_missing_data() {
    let game = fixture();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_ints
        .insert("isstaggering".into(), -3);
    let mut query = condition(675, -3.0);
    query.strings[0] = Some("IsStaggering".into());
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
    query.comparison = Comparison::Number(0.0);
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(false));
    query.comparison = Comparison::Number(-3.0);
    query.strings[0] = Some("MissingGraphVariable".into());
    assert_eq!(
        evaluate(&game, id(0x900), &query),
        Err(conditions::Error::Variable("MissingGraphVariable".into()))
    );
    query.strings[0] = Some("IsStaggering".into());
    assert_eq!(
        evaluate(&game, id(0x910), &query),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_path_speed_requires_a_pending_unarrived_waypoint() {
    let game = fixture();
    let positive = condition(683, 12.5);
    let zero = condition(683, 0.0);
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.speed = 12.5;
        actor.ai.path.push([1.0, 0.0, 0.0]);
        actor.ai.arrived = true;
    }
    assert_eq!(evaluate(&game, id(0x900), &zero), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(false));
    {
        let mut world = game.world_mut();
        world.actor(id(0x900)).unwrap().ai.arrived = false;
    }
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x910), &positive),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_allowed_to_fly_uses_actor_state_in_both_directions() {
    let game = fixture();
    let yes = condition(698, 1.0);
    let no = condition(698, 0.0);
    assert_eq!(evaluate(&game, id(0x900), &no), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &yes), Ok(false));
    game.world_mut().actor(id(0x900)).unwrap().allow_flying = true;
    assert_eq!(evaluate(&game, id(0x900), &yes), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &no), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x910), &yes),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_is_pathing_requires_pending_unarrived_path_and_actor_context() {
    let game = fixture();
    let yes = condition(704, 1.0);
    let no = condition(704, 0.0);
    assert_eq!(evaluate(&game, id(0x900), &no), Ok(true));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.path.push([1.0, 0.0, 0.0]);
        actor.ai.arrived = false;
    }
    assert_eq!(evaluate(&game, id(0x900), &yes), Ok(true));
    {
        let mut world = game.world_mut();
        world.actor(id(0x900)).unwrap().ai.arrived = true;
    }
    assert_eq!(evaluate(&game, id(0x900), &no), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x910), &yes),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_unavailable_motion_contracts_remain_explicit_for_all_contexts() {
    let game = fixture();
    for function in [644, 684, 730] {
        assert!(!conditions::supports_function(function));
        for expected in [0.0, 1.0] {
            let query = condition(function, expected);
            assert_eq!(
                evaluate(&game, id(0x900), &query),
                Err(conditions::Error::Unavailable(function))
            );
            assert_eq!(
                evaluate(&game, id(0x910), &query),
                Err(conditions::Error::Unavailable(function))
            );
        }
    }
}

#[test]
fn ai_67_mount_flags_are_idempotent_and_both_survive_world_reload() {
    let game = fixture();
    for _ in 0..2 {
        game.world_mut()
            .set_mount_rider(id(0x900), id(0x901), true)
            .unwrap();
    }
    assert!(game.world_mut().actor(id(0x900)).unwrap().riding_mount);
    assert!(game.world_mut().actor(id(0x901)).unwrap().being_ridden);
    let bytes = game.world_mut().save_state();
    game.world_mut()
        .set_mount_rider(id(0x900), id(0x901), false)
        .unwrap();
    game.world_mut().load_state(&bytes).unwrap();
    assert!(game.world_mut().actor(id(0x900)).unwrap().riding_mount);
    assert!(game.world_mut().actor(id(0x901)).unwrap().being_ridden);
    assert_eq!(evaluate(&game, id(0x901), &condition(714, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x901), &condition(714, 0.0)), Ok(false));
    game.world_mut()
        .set_mount_rider(id(0x900), id(0x901), false)
        .unwrap();
    assert_eq!(evaluate(&game, id(0x901), &condition(714, 0.0)), Ok(true));
}
