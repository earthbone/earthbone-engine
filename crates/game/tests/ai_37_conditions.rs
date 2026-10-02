// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-37 CTDA actor-value, skill-use and environment conditions.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{
    Game, World, conditions,
    perks::{EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x820));
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0x830).sub(Tag(*b"KWDA"), 0x820_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"PERK"), 0x840)
            .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 0, 0])
            .sub(Tag(*b"DATA"), [35, 2, 1])
            .sub(Tag(*b"PRKC"), [0])
            .sub(Tag(*b"CTDA"), condition_bytes(692, 3, 1.0))
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), 0.0_f32.to_le_bytes())
            .sub(Tag(*b"PRKF"), []),
    );
    let mut order = LoadOrder::new();
    order.add("Ai37.esm", plugin).unwrap();
    Game::new(order)
}

fn condition_bytes(function: u16, parameter: u32, expected: f32) -> [u8; 32] {
    let mut bytes = [0; 32];
    bytes[4..8].copy_from_slice(&expected.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn no_op_handler(_: &mut World, _: &EntryPointInvocation) {}

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

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    c: &Condition,
    skill_use: Option<conditions::SkillUseContext>,
    light_level: Option<f32>,
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
            skill_use,
            light_level,
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
fn ai_37_actor_value_percent_matches_actor_value_percentage() {
    let game = game();
    game.world_mut()
        .set_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(id(0x900), "health", 20.0)
        .unwrap();
    let yes = condition(640, 24, 0, 0.8);
    let no = condition(640, 24, 0, 0.7);
    assert!(conditions::supports_function(640));
    assert_eq!(evaluate(&game, id(0x900), &yes, None, None), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &no, None, None), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(640, 164, 0, 0.0), None, None),
        Err(conditions::Error::ActorValue(164))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &yes, None, None),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_37_advance_object_keyword_uses_matching_skill_event() {
    let game = game();
    let event = conditions::SkillUseContext {
        actor: id(0x900),
        action: 3,
        advance_object: Some(id(0x830)),
    };
    let yes = condition(691, 0x820, 0, 1.0);
    let no = condition(691, 0x820, 0, 0.0);
    assert!(conditions::supports_function(691));
    assert_eq!(
        evaluate(&game, id(0x900), &yes, Some(event), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &no, Some(event), None),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &yes, None, None),
        Err(conditions::Error::SkillUseContext(691))
    );
    let wrong_actor = conditions::SkillUseContext {
        actor: id(0x910),
        ..event
    };
    assert_eq!(
        evaluate(&game, id(0x900), &yes, Some(wrong_actor), None),
        Err(conditions::Error::SkillUseContext(691))
    );
}

#[test]
fn ai_37_advance_action_matches_creation_kit_action_number() {
    let game = game();
    let event = conditions::SkillUseContext {
        actor: id(0x900),
        action: 3,
        advance_object: None,
    };
    let yes = condition(692, 3, 0, 1.0);
    let no = condition(692, 2, 0, 1.0);
    assert!(conditions::supports_function(692));
    assert_eq!(
        evaluate(&game, id(0x900), &yes, Some(event), None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &no, Some(event), None),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &yes, None, None),
        Err(conditions::Error::SkillUseContext(692))
    );
    assert_eq!(
        evaluate(
            &game,
            id(0x900),
            &condition(692, 15, 0, 1.0),
            Some(event),
            None
        ),
        Err(conditions::Error::Parameter(692, 15))
    );
}

#[test]
fn ai_37_skill_event_reaches_perk_entrypoint_conditions() {
    let mut game = game();
    let actor = id(0x900);
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(0x840));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(35, no_op_handler);
    let event = conditions::SkillUseContext {
        actor,
        action: 3,
        advance_object: None,
    };
    let context = EntryPointContext {
        tabs: vec![actor],
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        skill_use: Some(event),
    };
    assert_eq!(dispatcher.dispatch(&mut game, actor, 35, &context), Ok(1));
}

#[test]
fn ai_37_light_level_uses_host_sample_with_ck_range() {
    let game = game();
    let yes = condition(711, 0, 0, 37.5);
    let no = condition(711, 0, 0, 38.0);
    assert!(conditions::supports_function(711));
    assert_eq!(evaluate(&game, id(0x900), &yes, None, Some(37.5)), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &no, None, Some(37.5)), Ok(false));
    assert_eq!(
        evaluate(
            &game,
            id(0x900),
            &condition(711, 0, 0, 0.0),
            None,
            Some(0.0)
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate(
            &game,
            id(0x900),
            &condition(711, 0, 0, 150.0),
            None,
            Some(150.0)
        ),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &yes, None, None),
        Err(conditions::Error::LightLevelContext(711))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &yes, None, Some(151.0)),
        Err(conditions::Error::LightLevelContext(711))
    );
}

#[test]
fn ai_37_over_encumbered_compares_inventory_and_carry_weights() {
    let game = game();
    game.world_mut()
        .set_actor_value(id(0x900), "inventoryweight", 101.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x900), "carryweight", 100.0)
        .unwrap();
    let yes = condition(734, 0, 0, 1.0);
    let no = condition(734, 0, 0, 0.0);
    assert!(conditions::supports_function(734));
    assert_eq!(evaluate(&game, id(0x900), &yes, None, None), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &no, None, None), Ok(false));
    game.world_mut()
        .set_actor_value(id(0x900), "inventoryweight", 100.0)
        .unwrap();
    assert_eq!(evaluate(&game, id(0x900), &no, None, None), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(734, 1, 0, 1.0), None, None),
        Err(conditions::Error::Parameter(734, 1))
    );
}

#[test]
fn ai_37_warmth_reports_missing_ck_contract() {
    let game = game();
    assert!(conditions::supports_function(735));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(735, 0, 0, 0.0), None, None),
        Err(conditions::Error::UnavailableContract(735))
    );
}
