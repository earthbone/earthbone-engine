// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-67 motion conditions backed by the current actor and package state.

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
    order.add("Ai67.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, parameter: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [parameter, 0],
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
fn ai_67_allowed_to_fly_reads_the_actor_flag_and_rejects_nonactors() {
    let game = fixture();
    let true_case = condition(698, 0, 1.0);
    let false_case = condition(698, 0, 0.0);
    assert!(conditions::supports_function(698));
    assert_eq!(evaluate(&game, id(0x900), &false_case), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &true_case), Ok(false));
    game.world_mut().actor(id(0x900)).unwrap().allow_flying = true;
    assert_eq!(evaluate(&game, id(0x900), &true_case), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x910), &true_case),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_is_pathing_tracks_pending_package_waypoints() {
    let game = fixture();
    let true_case = condition(704, 0, 1.0);
    let false_case = condition(704, 0, 0.0);
    assert!(conditions::supports_function(704));
    assert_eq!(evaluate(&game, id(0x900), &false_case), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &true_case), Ok(false));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.path.push([10.0, 0.0, 0.0]);
        actor.ai.arrived = false;
    }
    assert_eq!(evaluate(&game, id(0x900), &true_case), Ok(true));
    {
        let mut world = game.world_mut();
        world.actor(id(0x900)).unwrap().ai.arrived = true;
    }
    assert_eq!(evaluate(&game, id(0x900), &false_case), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x910), &true_case),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_pathing_current_speed_is_zero_without_a_path_and_reads_package_speed() {
    let game = fixture();
    let positive = condition(683, 0, 42.0);
    let negative = condition(683, 0, 0.0);
    assert!(conditions::supports_function(683));
    assert_eq!(evaluate(&game, id(0x900), &negative), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(false));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.ai.path.push([10.0, 0.0, 0.0]);
        actor.ai.arrived = false;
        actor.ai.speed = 42.0;
    }
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x910), &positive),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_conditions_without_runtime_contract_return_explicit_diagnostics() {
    let game = fixture();
    for function in [644, 684, 730] {
        assert!(!conditions::supports_function(function), "{function}");
        for expected in [0.0, 1.0] {
            let condition = condition(function, 0, expected);
            assert_eq!(
                evaluate(&game, id(0x900), &condition),
                Err(conditions::Error::Unavailable(function)),
                "{function}"
            );
            assert_eq!(
                evaluate(&game, id(0x910), &condition),
                Err(conditions::Error::Unavailable(function)),
                "invalid context for {function}"
            );
        }
    }
}

#[test]
fn ai_67_implemented_conditions_reject_unused_parameters() {
    let game = fixture();
    for function in [683, 698, 704, 714] {
        let condition = condition(function, 1, 0.0);
        assert_eq!(
            evaluate(&game, id(0x900), &condition),
            Err(conditions::Error::Parameter(function, 1)),
            "{function}"
        );
    }
}

#[test]
fn ai_67_is_being_ridden_reads_the_mount_actor_state() {
    let game = fixture();
    let ridden = condition(714, 0, 1.0);
    let not_ridden = condition(714, 0, 0.0);
    assert!(conditions::supports_function(714));
    assert_eq!(evaluate(&game, id(0x900), &not_ridden), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &ridden), Ok(false));

    game.world_mut().actor(id(0x900)).unwrap().being_ridden = true;
    assert_eq!(evaluate(&game, id(0x900), &ridden), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &not_ridden), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x910), &ridden),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_67_being_ridden_state_survives_world_save_load() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().being_ridden = true;
    let bytes = game.world_mut().save_state();
    game.world_mut().actor(id(0x900)).unwrap().being_ridden = false;
    game.world_mut().load_state(&bytes).unwrap();

    assert!(game.world_mut().actor(id(0x900)).unwrap().being_ridden);
    assert_eq!(
        evaluate(&game, id(0x900), &condition(714, 0, 1.0)),
        Ok(true)
    );
}
