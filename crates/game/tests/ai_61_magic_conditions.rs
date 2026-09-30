// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-61 magic CTDA predicates against authored spell and runtime state.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{combat::Hand, conditions, Game};

const ACTOR: u32 = 0x100;
const ACTOR_BASE: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const KEYWORD: u32 = 0x400;
const OTHER_KEYWORD: u32 = 0x401;
const PERK: u32 = 0x500;
const OTHER_PERK: u32 = 0x501;
const WEAPON: u32 = 0x600;
const WEAPON_SLOT: u32 = 0x601;
const POISON: u32 = 0x700;
const POISON_REF: u32 = 0x701;
const POTION: u32 = 0x702;
const POTION_REF: u32 = 0x703;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR_BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), ACTOR_BASE.to_le_bytes()));
    let mut mgef = vec![0; 152];
    put(&mut mgef, 12, 24);
    put(&mut mgef, 64, 0);
    put(&mut mgef, 80, 0);
    put(&mut mgef, 84, 0);
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), EFFECT)
            .sub(Tag(*b"DATA"), mgef)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 0);
    put(&mut spit, 20, 0);
    put(&mut spit, 32, PERK);
    let mut spell_effect_data = 0_f32.to_le_bytes().to_vec();
    spell_effect_data.extend_from_slice(&0_u32.to_le_bytes());
    spell_effect_data.extend_from_slice(&10_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), SPELL)
            .sub(Tag(*b"SPIT"), spit)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes())
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), spell_effect_data),
    );
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"KYWD"), OTHER_KEYWORD));
    plugin.push_top(Record::new(Tag(*b"PERK"), PERK));
    plugin.push_top(Record::new(Tag(*b"PERK"), OTHER_PERK));

    let mut weapon_data = vec![0; 10];
    weapon_data[8..10].copy_from_slice(&12_u16.to_le_bytes());
    let mut weapon_dnam = vec![0; 100];
    weapon_dnam[0] = 1;
    put(&mut weapon_dnam, 40, 1 << 13);
    put(&mut weapon_dnam, 76, 6);
    put(&mut weapon_dnam, 88, u32::MAX);
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), WEAPON)
            .sub(Tag(*b"DATA"), weapon_data)
            .sub(Tag(*b"DNAM"), weapon_dnam)
            .sub(Tag(*b"ETYP"), WEAPON_SLOT.to_le_bytes()),
    );
    plugin
        .push_top(Record::new(Tag(*b"EQUP"), WEAPON_SLOT).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));
    let mut slots = u32::from_le_bytes(*b"LHEQ").to_le_bytes().to_vec();
    slots.extend_from_slice(&0x610_u32.to_le_bytes());
    slots.extend_from_slice(&u32::from_le_bytes(*b"RHEQ").to_le_bytes());
    slots.extend_from_slice(&WEAPON_SLOT.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x611).sub(Tag(*b"DNAM"), slots));
    plugin.push_top(Record::new(Tag(*b"EQUP"), 0x610).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));

    for (form, flags) in [(POISON, 1 << 17), (POTION, 0)] {
        let mut ingestible_data = vec![0; 20];
        put(&mut ingestible_data, 4, flags);
        let mut effect = 0_f32.to_le_bytes().to_vec();
        effect.extend_from_slice(&0_u32.to_le_bytes());
        effect.extend_from_slice(&1_u32.to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"ALCH"), form)
                .sub(Tag(*b"ENIT"), ingestible_data)
                .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
                .sub(Tag(*b"EFIT"), effect),
        );
    }
    plugin
        .push_top(Record::new(Tag(*b"REFR"), POISON_REF).sub(Tag(*b"NAME"), POISON.to_le_bytes()));
    plugin
        .push_top(Record::new(Tag(*b"REFR"), POTION_REF).sub(Tag(*b"NAME"), POTION.to_le_bytes()));

    let mut order = LoadOrder::new();
    order.add("AI61.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(
    game: &Game,
    subject: u32,
    c: &Condition,
    magic_item: Option<u32>,
    dual_cast: Option<bool>,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(ACTOR_BASE),
            subject: id(subject),
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: magic_item.map(id),
            dual_cast,
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_61_spell_keyword_skill_and_casting_perk_match_the_spell_definition() {
    let game = fixture();
    for (function, parameter) in [(693, KEYWORD), (696, 24), (713, PERK)] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(
                &game,
                ACTOR,
                &condition(function, parameter, 1.0),
                Some(SPELL),
                None
            ),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                ACTOR,
                &condition(function, parameter, 0.0),
                Some(SPELL),
                None
            ),
            Ok(false)
        );
    }
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(693, OTHER_KEYWORD, 1.0),
            Some(SPELL),
            None
        ),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(696, 23, 1.0), Some(SPELL), None),
        Ok(false)
    );
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(713, OTHER_PERK, 1.0),
            Some(SPELL),
            None
        ),
        Ok(false)
    );
}

#[test]
fn ai_61_spell_predicates_reject_missing_or_wrong_context() {
    let game = fixture();
    for function in [693, 696, 713] {
        let parameter = if function == 696 {
            24
        } else if function == 713 {
            PERK
        } else {
            KEYWORD
        };
        assert_eq!(
            evaluate(
                &game,
                ACTOR,
                &condition(function, parameter, 1.0),
                None,
                None
            ),
            Err(conditions::Error::MagicContext(function))
        );
    }
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(693, 0xdead, 1.0),
            Some(SPELL),
            None
        ),
        Err(conditions::Error::Form(id(0xdead)))
    );
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(713, 0xdead, 1.0),
            Some(SPELL),
            None
        ),
        Err(conditions::Error::Form(id(0xdead)))
    );
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(696, u32::MAX, 1.0),
            Some(SPELL),
            None
        ),
        Err(conditions::Error::ActorValue(u32::MAX))
    );
}

#[test]
fn ai_61_active_effect_keyword_tracks_live_effects() {
    let game = fixture();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(ACTOR)),
            Some(id(ACTOR)),
            &mut |_, _| soe_papyrus::ObjectId(1),
        )
        .unwrap();
    assert_eq!(
        evaluate(&game, ACTOR, &condition(699, KEYWORD, 1.0), None, None),
        Ok(true)
    );
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(699, OTHER_KEYWORD, 1.0),
            None,
            None
        ),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(699, 0xdead, 1.0), None, None),
        Err(conditions::Error::Form(id(0xdead)))
    );
}

#[test]
fn ai_61_bound_weapon_uses_casting_source_hand_and_bound_flag() {
    let game = fixture();
    game.world_mut().add_item(id(ACTOR), id(WEAPON), 1);
    game.world_mut()
        .equip_hand_weapon(id(ACTOR), Hand::Right, id(WEAPON))
        .unwrap();
    assert_eq!(
        evaluate(&game, ACTOR, &condition(706, 1, 1.0), None, None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(706, 0, 1.0), None, None),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(706, 2, 1.0), None, None),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(706, 3, 1.0), None, None),
        Err(conditions::Error::Parameter(706, 3))
    );
    assert_eq!(
        evaluate(&game, POISON_REF, &condition(706, 1, 1.0), None, None),
        Err(conditions::Error::Form(id(POISON_REF)))
    );
}

#[test]
fn ai_61_is_poison_checks_ingestible_base_and_requires_no_parameters() {
    let game = fixture();
    assert!(conditions::supports_function(721));
    assert_eq!(
        evaluate(&game, POISON_REF, &condition(721, 0, 1.0), None, None),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, POTION_REF, &condition(721, 0, 1.0), None, None),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(721, 0, 1.0), None, None),
        Err(conditions::Error::Form(id(ACTOR_BASE)))
    );
    assert_eq!(
        evaluate(&game, POISON_REF, &condition(721, 1, 1.0), None, None),
        Err(conditions::Error::Parameter(721, 1))
    );
}

#[test]
fn ai_61_effect_was_dual_cast_requires_explicit_cast_context() {
    let game = fixture();
    assert!(conditions::supports_function(724));
    assert_eq!(
        evaluate(&game, ACTOR, &condition(724, 0, 1.0), None, Some(true)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(724, 0, 1.0), None, Some(false)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(724, 0, 1.0), None, None),
        Err(conditions::Error::MagicContext(724))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(724, 1, 1.0), None, Some(false)),
        Err(conditions::Error::Parameter(724, 1))
    );
}

#[test]
fn ai_61_advance_skill_reports_the_missing_runtime_contract() {
    let game = fixture();
    assert!(!conditions::supports_function(681));
    assert_eq!(
        evaluate(&game, ACTOR, &condition(681, 24, 1.0), None, None),
        Err(conditions::Error::Function(681))
    );
}
