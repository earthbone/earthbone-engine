// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-59 magic and alchemy CTDA predicates.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{
    combat::Hand,
    conditions,
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const OWNER: u32 = 0x800;
const ACTOR: u32 = 0x900;
const OTHER: u32 = 0x901;
const EFFECT: u32 = 0x200;
const OTHER_EFFECT: u32 = 0x201;
const SPELL: u32 = 0x300;
const SHOUT: u32 = 0x301;
const POTION: u32 = 0x302;
const KEYWORD: u32 = 0x400;
const PERK: u32 = 0x880;
const ENTRY_POINT: u8 = 35;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn ctda(function: u16, parameter: u32, expected: f32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data
}

fn noop_handler(_: &mut World, _: &EntryPointInvocation) {}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    for actor in [ACTOR, OTHER] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor)
                .sub(Tag(*b"NAME"), OWNER.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
    }
    for (effect, keyword) in [(EFFECT, KEYWORD), (OTHER_EFFECT, 0x402)] {
        let mut data = vec![0; 152];
        put(&mut data, 64, 0);
        put(&mut data, 68, 24);
        plugin.push_top(
            Record::new(Tag(*b"MGEF"), effect)
                .sub(Tag(*b"DATA"), data)
                .sub(Tag(*b"KWDA"), keyword.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"KYWD"), 0x402));
    plugin.push_top(Record::new(Tag(*b"SHOU"), SHOUT));
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 8, 0);
    plugin.push_top(Record::new(Tag(*b"SPEL"), SPELL).sub(Tag(*b"SPIT"), spell_data));
    let alch_data = vec![0; 20];
    let mut effect_data = 10.0_f32.to_le_bytes().to_vec();
    effect_data.extend_from_slice(&0_u32.to_le_bytes());
    effect_data.extend_from_slice(&10_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"ALCH"), POTION)
            .sub(Tag(*b"ENIT"), alch_data)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), effect_data),
    );
    plugin.push_top(
        Record::new(Tag(*b"PERK"), PERK)
            .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 0, 0])
            .sub(Tag(*b"DATA"), [ENTRY_POINT, 2, 1])
            .sub(Tag(*b"PRKC"), [0])
            .sub(Tag(*b"CTDA"), ctda(500, 0, 1.0))
            .sub(Tag(*b"CTDA"), ctda(501, KEYWORD, 1.0))
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), 0.0_f32.to_le_bytes())
            .sub(Tag(*b"PRKF"), []),
    );
    let mut order = LoadOrder::new();
    order.add("AI59.esm", plugin).unwrap();
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
    context: conditions::Context,
    c: &Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, c)
}

fn context() -> conditions::Context {
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
    }
}

#[test]
fn ai_59_active_effect_and_spell_target_match_live_applications() {
    let mut game = fixture();
    game.world_mut().add_item(id(ACTOR), id(POTION), 1);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |_, _| {
            game.vm.create_object_of("EffectProbe", "ActiveMagicEffect")
        })
        .unwrap();
    for (function, parameter) in [(214, EFFECT), (223, POTION)] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, context(), &condition(function, parameter, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                context(),
                &condition(
                    function,
                    if function == 214 { OTHER_EFFECT } else { SPELL },
                    1.0
                )
            ),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, context(), &condition(function, parameter, 0.0)),
            Ok(false)
        );
    }
    assert_eq!(
        evaluate(&game, context(), &condition(214, SHOUT, 1.0)),
        Err(conditions::Error::Form(id(SHOUT)))
    );
}

#[test]
fn ai_59_shout_and_equipped_spell_conditions_read_actor_state() {
    let game = fixture();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.shouts.insert(id(SHOUT));
        actor.spells.insert(id(SPELL));
        world
            .equip_hand_spell(id(ACTOR), Hand::Right, id(SPELL))
            .unwrap();
    }
    for (function, parameter) in [(378, SHOUT), (570, 1)] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, context(), &condition(function, parameter, 1.0)),
            Ok(true)
        );
        assert_eq!(
            evaluate(&game, context(), &condition(function, parameter, 0.0)),
            Ok(false)
        );
    }
    assert_eq!(
        evaluate(&game, context(), &condition(378, SPELL, 1.0)),
        Err(conditions::Error::Form(id(SPELL)))
    );
    assert_eq!(
        evaluate(&game, context(), &condition(570, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, context(), &condition(570, 3, 0.0)),
        Err(conditions::Error::Parameter(570, 3))
    );
}

#[test]
fn ai_59_alchemy_entry_point_conditions_require_and_match_context() {
    let game = fixture();
    let poison = condition(500, 0, 1.0);
    let mut making_poison = context();
    making_poison.alchemy_making_poison = Some(true);
    assert!(conditions::supports_function(500));
    assert_eq!(evaluate(&game, making_poison, &poison), Ok(true));
    making_poison.alchemy_making_poison = Some(false);
    assert_eq!(evaluate(&game, making_poison, &poison), Ok(false));
    assert_eq!(
        evaluate(&game, context(), &poison),
        Err(conditions::Error::AlchemyContext(500))
    );

    let keyword = condition(501, KEYWORD, 1.0);
    let mut effect_context = context();
    effect_context.alchemy_effect = Some(id(EFFECT));
    assert!(conditions::supports_function(501));
    assert_eq!(evaluate(&game, effect_context, &keyword), Ok(true));
    assert_eq!(
        evaluate(&game, effect_context, &condition(501, 0x402, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, context(), &keyword),
        Err(conditions::Error::AlchemyContext(501))
    );
    assert_eq!(
        evaluate(&game, context(), &condition(501, SPELL, 1.0)),
        Err(conditions::Error::Form(id(SPELL)))
    );
}

#[test]
fn ai_59_unknown_runtime_contracts_report_typed_diagnostics() {
    let game = fixture();
    for function in [101, 552] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, context(), &condition(function, 0, 1.0)),
            Err(conditions::Error::Contract(function))
        );
    }
}

#[test]
fn ai_59_perk_dispatch_passes_alchemy_entry_point_context() {
    let mut game = fixture();
    let actor = id(ACTOR);
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY_POINT, noop_handler);
    let mut context = EntryPointContext {
        tabs: vec![actor],
        target: None,
        quest: None,
        tempering_item: None,
        alchemy_making_poison: Some(true),
        alchemy_effect: Some(id(EFFECT)),
        skill_use: None,
    };
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY_POINT, &context),
        Ok(1)
    );
    context.alchemy_making_poison = Some(false);
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY_POINT, &context),
        Ok(0)
    );
    context.alchemy_making_poison = Some(true);
    context.alchemy_effect = None;
    assert!(matches!(
        dispatcher.dispatch(&mut game, actor, ENTRY_POINT, &context),
        Err(DispatchError::Condition(perk, conditions::Error::AlchemyContext(501)))
            if perk == id(PERK)
    ));
}
