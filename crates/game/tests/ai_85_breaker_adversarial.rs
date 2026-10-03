// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-85 world condition contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"WATR"), 0x812));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x912).sub(Tag(*b"NAME"), 0x812_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai85Breaker.esm", plugin).unwrap();
    Game::new(order)
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
fn ai_85_restrained_reads_actor_state_and_rejects_non_actor_or_missing_subjects() {
    let game = fixture();
    let query = condition(244, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(false));
    game.world_mut().actor(id(0x900)).unwrap().restrained = true;
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(244, 0.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x910), &query), Err(conditions::Error::Form(id(0x910))));
    assert_eq!(evaluate(&game, id(0xdead), &query), Err(conditions::Error::Form(id(0xdead))));
}

#[test]
fn ai_85_water_object_uses_the_reference_base_record_and_validates_context() {
    let game = fixture();
    assert_eq!(evaluate(&game, id(0x912), &condition(304, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x912), &condition(304, 0.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x910), &condition(304, 1.0)), Ok(false));
    assert_eq!(evaluate(&game, id(0x910), &condition(304, 0.0)), Ok(true));
    assert_eq!(evaluate(&game, id(0x812), &condition(304, 1.0)), Err(conditions::Error::Form(id(0x812))));
    assert_eq!(evaluate(&game, id(0xdead), &condition(304, 1.0)), Err(conditions::Error::Form(id(0xdead))));
}

#[test]
fn ai_85_documented_parameterless_functions_reject_both_slots_and_missing_targets() {
    let game = fixture();
    for function in [244, 304, 716] {
        let mut first = condition(function, 0.0);
        first.parameters[0] = 1;
        assert_eq!(evaluate(&game, id(0x900), &first), Err(conditions::Error::Parameter(function, 1)));
        let mut second = condition(function, 0.0);
        second.parameters[1] = 1;
        assert_eq!(evaluate(&game, id(0x900), &second), Err(conditions::Error::Parameter(function, 1)));
        let mut missing_target = condition(function, 0.0);
        missing_target.run_on = 1;
        assert_eq!(evaluate(&game, id(0x900), &missing_target), Err(conditions::Error::Target(1)));
    }
}

#[test]
fn ai_85_real_hours_uses_elapsed_real_time_not_scaled_game_time() {
    let game = fixture();
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.0)), Ok(true));
    game.world_mut().clock.timescale = 20.0;
    game.world_mut().advance_clock(180.0);
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.05)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &condition(716, 0.1)), Ok(false));
}

#[test]
fn ai_85_restraint_and_real_clock_queries_survive_engine_save_load() {
    let original = fixture();
    original.world_mut().actor(id(0x900)).unwrap().restrained = true;
    original.world_mut().advance_clock(5_400.0);
    let saved = original.save_state().unwrap();

    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(evaluate(&restored, id(0x900), &condition(244, 1.0)), Ok(true));
    assert_eq!(evaluate(&restored, id(0x900), &condition(716, 1.5)), Ok(true));
    assert_eq!(evaluate(&restored, id(0x900), &condition(716, 2.0)), Ok(false));
}

#[test]
fn ai_85_undocumented_functions_remain_explicit_diagnostics() {
    let game = fixture();
    for function in [255, 265, 266, 267, 332] {
        assert!(!conditions::supports_function(function));
        assert_eq!(evaluate(&game, id(0x900), &condition(function, 0.0)), Err(conditions::Error::Function(function)));
        assert_eq!(evaluate(&game, id(0x910), &condition(function, 1.0)), Err(conditions::Error::Function(function)));
        let mut missing_target = condition(function, 1.0);
        missing_target.run_on = 1;
        assert_eq!(evaluate(&game, id(0x900), &missing_target), Err(conditions::Error::Target(1)));
    }
}
