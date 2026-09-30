// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-51 equipment condition functions over equipment and item context.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{
    conditions,
    inventory::*,
    perks::{DispatchError, EntryPointContext, EntryPointDispatcher, EntryPointInvocation},
    Game, World,
};

const ACTOR: u32 = 0x800;
const SWORD: u32 = 0x801;
const GREAT_SWORD: u32 = 0x802;
const KEYWORD: u32 = 0x803;
const ITEM_REF: u32 = 0x804;
const PERK: u32 = 0x805;
const ENTRY: u8 = 201;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x810));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"WEAP"), SWORD).sub(Tag(*b"DNAM"), [1]));
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), GREAT_SWORD)
            .sub(Tag(*b"DNAM"), [5])
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), ITEM_REF).sub(Tag(*b"NAME"), GREAT_SWORD.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"PERK"), PERK)
            .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
            .sub(Tag(*b"PRKE"), [2, 0, 0])
            .sub(Tag(*b"DATA"), [ENTRY, 0, 1])
            .sub(Tag(*b"PRKC"), [0])
            .sub(Tag(*b"CTDA"), condition_bytes(660, KEYWORD))
            .sub(Tag(*b"EPFT"), [1])
            .sub(Tag(*b"EPFD"), 0.0_f32.to_le_bytes())
            .sub(Tag(*b"PRKF"), []),
    );
    let mut order = LoadOrder::new();
    order.add("Ai51.esm", plugin).unwrap();
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

fn condition_bytes(function: u16, parameter: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn mark_perk(world: &mut World, invocation: &EntryPointInvocation) {
    let current = world
        .actor(invocation.actor)
        .unwrap()
        .values
        .get("health")
        .current();
    world
        .set_actor_value(invocation.actor, "health", current + 1.0)
        .unwrap();
}

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    item: Option<GlobalFormId>,
    condition: &Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(ACTOR),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: item,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        condition,
    )
}

#[test]
fn ai_51_two_handed_equipment_matches_authored_weapon_type_and_validates_context() {
    let game = game();
    let mut world = game.world_mut();
    world.actor(id(ACTOR)).unwrap().equipped.insert(id(SWORD));
    drop(world);
    let two_handed = condition(636, 0, 1.0);
    assert!(conditions::supports_function(636));
    assert_eq!(evaluate(&game, id(ACTOR), None, &two_handed), Ok(false));
    let one_handed = condition(636, 0, 0.0);
    assert_eq!(evaluate(&game, id(ACTOR), None, &one_handed), Ok(true));
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .equipped
        .insert(id(GREAT_SWORD));
    assert_eq!(evaluate(&game, id(ACTOR), None, &two_handed), Ok(true));
    assert_eq!(evaluate(&game, id(ACTOR), None, &one_handed), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x999), None, &two_handed),
        Err(conditions::Error::Form(id(0x999)))
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), None, &condition(636, 1, 1.0)),
        Err(conditions::Error::Parameter(636, 1))
    );
}

#[test]
fn ai_51_tempering_keyword_uses_perk_item_context_and_keyword_form() {
    let game = game();
    let has_keyword = condition(660, KEYWORD, 1.0);
    let lacks_keyword = condition(660, KEYWORD, 0.0);
    assert!(conditions::supports_function(660));
    assert_eq!(
        evaluate(&game, id(ACTOR), Some(id(GREAT_SWORD)), &has_keyword),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), Some(id(SWORD)), &has_keyword),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), Some(id(SWORD)), &lacks_keyword),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), None, &has_keyword),
        Err(conditions::Error::TemperingContext)
    );
    assert_eq!(
        evaluate(
            &game,
            id(ACTOR),
            Some(id(GREAT_SWORD)),
            &condition(660, SWORD, 1.0)
        ),
        Err(conditions::Error::Form(id(SWORD)))
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), Some(id(0x999)), &has_keyword),
        Err(conditions::Error::Form(id(0x999)))
    );
}

#[test]
fn ai_51_perk_dispatch_passes_tempering_item_into_entry_conditions() {
    let mut game = game();
    let actor = id(ACTOR);
    game.world_mut()
        .actor(actor)
        .unwrap()
        .perks
        .insert(id(PERK));
    game.world_mut()
        .set_actor_value(actor, "health", 100.0)
        .unwrap();
    let mut dispatcher = EntryPointDispatcher::default();
    dispatcher.register(ENTRY, mark_perk);
    let mut context = EntryPointContext {
        tabs: vec![id(ACTOR)],
        target: None,
        quest: None,
        tempering_item: Some(id(GREAT_SWORD)),
        alchemy_making_poison: None,
        alchemy_effect: None,
    };
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context),
        Ok(1)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 101.0);

    context.tempering_item = Some(id(SWORD));
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context),
        Ok(0)
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 101.0);

    context.tempering_item = None;
    assert_eq!(
        dispatcher.dispatch(&mut game, actor, ENTRY, &context),
        Err(DispatchError::Condition(
            id(PERK),
            conditions::Error::TemperingContext
        ))
    );
}

#[test]
fn ai_51_replaced_item_type_reports_missing_semantic_contract() {
    let game = game();
    assert!(!conditions::supports_function(664));
    for expected in [0.0, 1.0] {
        assert_eq!(
            evaluate(&game, id(ACTOR), None, &condition(664, 0, expected)),
            Err(conditions::Error::Function(664))
        );
        assert_eq!(
            evaluate(&game, id(0x999), None, &condition(664, 0, expected)),
            Err(conditions::Error::Function(664))
        );
    }
}

#[test]
fn ai_51_item_health_percent_reads_item_extra_and_follows_ck_defaults() {
    let game = game();
    let mut world = game.world_mut();
    let reference = world.reference(id(ITEM_REF)).unwrap();
    reference.item = Some(
        Stack::new(
            1,
            vec![Extra {
                kind: 37,
                fields: vec![Field::FloatBits(1.5_f32.to_bits())],
            }],
        )
        .unwrap(),
    );
    drop(world);
    let legendary = condition(723, 0, 1.5);
    assert!(conditions::supports_function(723));
    assert_eq!(evaluate(&game, id(ITEM_REF), None, &legendary), Ok(true));
    assert_eq!(
        evaluate(&game, id(ITEM_REF), None, &condition(723, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), None, &condition(723, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ITEM_REF), None, &condition(723, 1, 1.5)),
        Err(conditions::Error::Parameter(723, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x999), None, &legendary),
        Err(conditions::Error::Form(id(0x999)))
    );
}

#[test]
fn ai_51_item_health_percent_preserves_damaged_values_and_rejects_bad_extras() {
    let game = game();
    game.world_mut().reference(id(ITEM_REF)).unwrap().item = Some(
        Stack::new(
            1,
            vec![Extra {
                kind: 37,
                fields: vec![Field::FloatBits(0.75_f32.to_bits())],
            }],
        )
        .unwrap(),
    );
    assert_eq!(
        evaluate(&game, id(ITEM_REF), None, &condition(723, 0, 0.75)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ITEM_REF), None, &condition(723, 0, 1.0)),
        Ok(false)
    );
    game.world_mut().reference(id(ITEM_REF)).unwrap().item = Some(
        Stack::new(
            1,
            vec![Extra {
                kind: 37,
                fields: vec![Field::Short(150)],
            }],
        )
        .unwrap(),
    );
    assert_eq!(
        evaluate(&game, id(ITEM_REF), None, &condition(723, 0, 1.0)),
        Err(conditions::Error::ItemHealth)
    );
}
