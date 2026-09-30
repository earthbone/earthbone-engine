// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-76 reference state predicates and explicit diagnostics without a contract.

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
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0xA00));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai76.esm", plugin).unwrap();
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
fn ai_76_is_protected_reads_the_subject_base_override() {
    let game = fixture();
    game.world_mut().actor_base(id(0x800)).unwrap().protected = true;
    assert!(conditions::supports_function(476));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(476, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(476, 0, 0.0)),
        Ok(false)
    );
    game.world_mut().actor_base(id(0x800)).unwrap().protected = false;
    assert_eq!(
        evaluate(&game, id(0x900), &condition(476, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(476, 0, 0.0)),
        Err(conditions::Error::Form(id(0x910)))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(476, 1, 0.0)),
        Err(conditions::Error::Parameter(476, 1))
    );
}

#[test]
fn ai_76_player_knows_reads_player_spell_knowledge() {
    let game = fixture();
    assert!(conditions::supports_function(493));
    let query = condition(493, 0xA00, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(false));
    let player = game.world_mut().player();
    game.world_mut()
        .actor(player)
        .unwrap()
        .spells
        .insert(id(0xA00));
    assert_eq!(evaluate(&game, id(0x900), &query), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(493, 0xA00, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(493, 0xFFF, 1.0)),
        Err(conditions::Error::Form(id(0xFFF)))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(493, 0x810, 1.0)),
        Err(conditions::Error::StateContract(493))
    );
    let mut malformed = condition(493, 0xA00, 1.0);
    malformed.parameters[1] = 1;
    assert_eq!(
        evaluate(&game, id(0x900), &malformed),
        Err(conditions::Error::Parameter(493, 1))
    );
}

#[test]
fn ai_76_uncontracted_state_queries_return_explicit_diagnostics() {
    let game = fixture();
    for function in [503, 521, 547, 554, 555, 572] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 1.0)),
            Err(conditions::Error::Function(function)),
            "function {function} must not invent unavailable runtime state"
        );
        let mut invalid_context = condition(function, 0, 1.0);
        invalid_context.run_on = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &invalid_context),
            Err(conditions::Error::Target(1)),
            "function {function} must diagnose missing target context"
        );
    }
}

#[test]
fn ai_76_supported_queries_reject_missing_run_on_context() {
    let game = fixture();
    for mut condition in [condition(476, 0, 1.0), condition(493, 0xA00, 1.0)] {
        condition.run_on = 1;
        assert_eq!(
            evaluate(&game, id(0x900), &condition),
            Err(conditions::Error::Target(1))
        );
    }
}
