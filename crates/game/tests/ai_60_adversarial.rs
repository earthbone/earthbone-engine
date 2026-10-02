// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-60 condition checks use selected spell slots, equipped shouts and held casts.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

const OWNER: u32 = 0x800;
const ACTOR: u32 = 0x801;
const SPELL: u32 = 0x802;
const OTHER_SPELL: u32 = 0x803;
const EFFECT: u32 = 0x804;
const KEYWORD: u32 = 0x805;
const SHOUT: u32 = 0x806;
const OTHER_KEYWORD: u32 = 0x807;
const OTHER_SHOUT: u32 = 0x808;
const OBJECT: u32 = 0x809;
const REFERENCE: u32 = 0x80a;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), OWNER.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(Record::new(Tag(*b"KYWD"), OTHER_KEYWORD));

    let mut effect_data = vec![0; 152];
    effect_data[64..68].copy_from_slice(&0_u32.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24_u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&2_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"MGEF"), EFFECT)
            .sub(Tag(*b"DATA"), effect_data)
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    for object in [SPELL, OTHER_SPELL] {
        let mut spell_data = vec![0; 36];
        spell_data[16..20].copy_from_slice(&2_u32.to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"SPEL"), object)
                .sub(Tag(*b"SPIT"), spell_data)
                .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
                .sub(Tag(*b"EFIT"), vec![0; 12]),
        );
    }
    plugin.push_top(Record::new(Tag(*b"SHOU"), SHOUT));
    plugin.push_top(Record::new(Tag(*b"SHOU"), OTHER_SHOUT));
    plugin.push_top(Record::new(Tag(*b"STAT"), OBJECT));
    plugin.push_top(Record::new(Tag(*b"REFR"), REFERENCE).sub(Tag(*b"NAME"), OBJECT.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai60.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        assert!(world.drive_actor(id(ACTOR)));
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.spells.extend([id(SPELL), id(OTHER_SPELL)]);
        actor.left_spell = Some(id(SPELL));
        actor.right_spell = Some(id(OTHER_SPELL));
        actor.equipped_shout = Some(id(SHOUT));
        world.set_actor_value(id(ACTOR), "magicka", 100.0).unwrap();
    }
    game
}

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

fn evaluate(game: &Game, c: &Condition) -> Result<bool, conditions::Error> {
    evaluate_subject(game, id(ACTOR), c)
}

fn evaluate_subject(
    game: &Game,
    subject: GlobalFormId,
    c: &Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(OWNER),
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
fn ai_60_known_magic_conditions_match_spell_slots_shout_and_cast_state() {
    let mut game = fixture();
    {
        let shared = game.shared_world();
        shared
            .borrow_mut()
            .begin_concentration_cast(id(SPELL), id(ACTOR), id(ACTOR), None, &mut |name, class| {
                game.vm.create_object_of(name, class)
            })
            .unwrap();
    }
    for (function, first, second, expected) in [
        (571, 0, 0, 2.0),
        (579, SHOUT, 0, 1.0),
        (595, SPELL, 0, 1.0),
        (596, 0, KEYWORD, 1.0),
        (632, 0, 0, 1.0),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, &condition(function, first, second, expected)),
            Ok(true),
            "function {function}"
        );
        let (negative_first, negative_second) = if function == 571 {
            (3, 0)
        } else if function == 579 {
            (OTHER_SHOUT, 0)
        } else if function == 595 {
            (OTHER_SPELL, 0)
        } else {
            (0, 0)
        };
        if function == 632 {
            assert!(game.world_mut().release_concentration_cast());
        }
        if function == 596 {
            continue;
        }
        assert_eq!(
            evaluate(
                &game,
                &condition(function, negative_first, negative_second, expected)
            ),
            Ok(false),
            "function {function}"
        );
    }
    assert_eq!(
        evaluate(&game, &condition(596, 0, OTHER_KEYWORD, 1.0)),
        Ok(false)
    );
    assert_eq!(evaluate(&game, &condition(571, 0, 0, 2.0)), Ok(false));
    assert_eq!(evaluate(&game, &condition(595, SPELL, 0, 1.0)), Ok(false));
}

#[test]
fn ai_60_magic_conditions_reject_bad_source_parameters_and_forms() {
    let game = fixture();
    for (function, first, second) in [(571, 4, 0), (596, 4, KEYWORD), (595, SPELL, 4)] {
        assert_eq!(
            evaluate(&game, &condition(function, first, second, 0.0)),
            Err(conditions::Error::Parameter(function, 4))
        );
    }
    assert!(matches!(
        evaluate(&game, &condition(579, 0xfff, 0, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, &condition(595, 0xfff, 0, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, &condition(596, 0, 0xfff, 0.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_60_untracked_ward_dual_cast_and_voice_state_return_specific_diagnostics() {
    let game = fixture();
    for (function, parameter, detail) in [
        (602, 1, "ward animation state is not supplied by the host"),
        (627, 0, "dual-cast animation state is not tracked"),
        (676, 0, "active voice variation is not tracked"),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, &condition(function, parameter, 0, 0.0)),
            Err(conditions::Error::UnavailableContractReason(
                function, detail
            ))
        );
    }
    assert_eq!(
        evaluate(&game, &condition(602, 3, 0, 0.0)),
        Err(conditions::Error::Parameter(602, 3))
    );
    assert_eq!(
        evaluate(&game, &condition(627, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(627, 1))
    );
    assert_eq!(
        evaluate(&game, &condition(676, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(676, 1))
    );
}

#[test]
fn ai_60_magic_conditions_reject_nonactor_subjects() {
    let game = fixture();
    for (function, first, second) in [
        (571, 0, 0),
        (579, SHOUT, 0),
        (595, SPELL, 0),
        (596, 0, KEYWORD),
        (602, 1, 0),
        (627, 0, 0),
        (632, 0, 0),
        (676, 0, 0),
    ] {
        assert_eq!(
            evaluate_subject(
                &game,
                id(REFERENCE),
                &condition(function, first, second, 0.0)
            ),
            Err(conditions::Error::Form(id(REFERENCE))),
            "function {function}"
        );
    }
}
