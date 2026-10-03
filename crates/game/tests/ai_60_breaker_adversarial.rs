// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-60 casting and host animation conditions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const OWNER: u32 = 0x800;
const ACTOR: u32 = 0x801;
const SPELL: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const EFFECT: u32 = 0x804;
const SHOUT: u32 = 0x805;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), OWNER.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    let mut effect_data = vec![0; 152];
    effect_data[64..68].copy_from_slice(&0_u32.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24_u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), EFFECT)
            .sub(Tag(*b"DATA"), effect_data)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spell_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), vec![0; 12]),
    );
    plugin.push_top(Record::new(Tag(*b"SHOU"), SHOUT));
    let mut order = LoadOrder::new();
    order.add("Ai60Breaker.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        assert!(world.drive_actor(id(ACTOR)));
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.left_spell = Some(id(SPELL));
        actor.equipped_shout = Some(id(SHOUT));
    }
    game
}

fn condition(function: u16, first: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [first, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(
    game: &Game,
    function: u16,
    first: u32,
    expected: f32,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(OWNER),
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
        &condition(function, first, expected),
    )
}

#[test]
fn ai_60_fire_and_forget_cast_is_current_until_released_or_interrupted() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        world.begin_spell_cast(id(ACTOR), id(SPELL), None).unwrap();
    }
    let active = [
        evaluate(&game, 571, 0, 1.0),
        evaluate(&game, 595, SPELL, 1.0),
        evaluate(&game, 632, 0, 1.0),
    ];
    assert!(game.world_mut().interrupt_spell_cast(id(ACTOR)));
    let inactive = [
        evaluate(&game, 571, 0, 1.0),
        evaluate(&game, 595, SPELL, 1.0),
        evaluate(&game, 632, 0, 1.0),
    ];
    assert_eq!(
        active,
        [Ok(true), Ok(true), Ok(true)],
        "active pending cast"
    );
    assert_eq!(
        inactive,
        [Ok(false), Ok(false), Ok(false)],
        "after interrupt"
    );
}

#[test]
fn ai_60_ward_enum_transitions_compare_all_defined_states_and_clear() {
    let game = fixture();
    for state in 0..=2 {
        game.world_mut()
            .set_ward_state(id(ACTOR), Some(state))
            .unwrap();
        assert_eq!(evaluate(&game, 602, state, 1.0), Ok(true));
        for other in (0..=2).filter(|other| *other != state) {
            assert_eq!(evaluate(&game, 602, other, 1.0), Ok(false));
        }
    }
    game.world_mut().set_ward_state(id(ACTOR), None).unwrap();
    assert!(matches!(
        evaluate(&game, 602, 0, 1.0),
        Err(conditions::Error::UnavailableContractReason(602, _))
    ));
}

#[test]
fn ai_60_dual_cast_false_is_distinct_from_unreported_and_updates_repeatedly() {
    let game = fixture();
    for value in [Some(true), Some(false), Some(true), None] {
        game.world_mut().set_dual_casting(id(ACTOR), value).unwrap();
        let result = evaluate(&game, 627, 0, 1.0);
        match value {
            Some(value) => assert_eq!(result, Ok(value)),
            None => assert!(matches!(
                result,
                Err(conditions::Error::UnavailableContractReason(627, _))
            )),
        }
    }
}

#[test]
fn ai_60_shout_variation_changes_and_clear_do_not_leave_stale_values() {
    let game = fixture();
    for value in [2, 1, 0] {
        game.world_mut()
            .set_shout_variation(id(ACTOR), Some(value))
            .unwrap();
        assert_eq!(evaluate(&game, 676, 0, value as f32), Ok(true));
        if value < 2 {
            assert_eq!(evaluate(&game, 676, 0, (value + 1) as f32), Ok(false));
        }
    }
    game.world_mut()
        .set_shout_variation(id(ACTOR), None)
        .unwrap();
    assert!(matches!(
        evaluate(&game, 676, 0, 0.0),
        Err(conditions::Error::UnavailableContractReason(676, _))
    ));
}

#[test]
fn ai_60_host_animation_signals_are_cleared_by_world_save_reload() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        world.set_ward_state(id(ACTOR), Some(1)).unwrap();
        world.set_dual_casting(id(ACTOR), Some(true)).unwrap();
        world.set_shout_variation(id(ACTOR), Some(2)).unwrap();
        let bytes = world.save_state();
        world.load_state(&bytes).unwrap();
    }
    for (function, parameter) in [(602, 1), (627, 0), (676, 0)] {
        assert!(matches!(
            evaluate(&game, function, parameter, 1.0),
            Err(conditions::Error::UnavailableContractReason(found, _)) if found == function
        ));
    }
}

#[test]
fn ai_60_host_animation_setters_reject_missing_actor_without_panicking() {
    let game = fixture();
    let mut world = game.world_mut();
    assert!(world.set_ward_state(id(0x8ff), Some(1)).is_err());
    assert!(world.set_dual_casting(id(0x8ff), Some(true)).is_err());
    assert!(world.set_shout_variation(id(0x8ff), Some(1)).is_err());
}

#[test]
fn ai_60_rejected_ward_state_does_not_clear_last_host_state() {
    let game = fixture();
    game.world_mut().set_ward_state(id(ACTOR), Some(2)).unwrap();
    assert!(game
        .world_mut()
        .set_ward_state(id(ACTOR), Some(u32::MAX))
        .is_err());
    assert_eq!(evaluate(&game, 602, 2, 1.0), Ok(true));
    assert_eq!(evaluate(&game, 602, 1, 1.0), Ok(false));
}
