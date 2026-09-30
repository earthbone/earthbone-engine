// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-80 player identity and faction CTDA contracts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{combat::MeleeHit, conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn form(object: u32) -> Vec<u8> {
    object.to_le_bytes().to_vec()
}

fn faction(object: u32, rank: i8) -> Vec<u8> {
    [object.to_le_bytes().as_slice(), &[rank as u8]].concat()
}

fn weapon(object: u32) -> Record {
    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&6_i32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn add_combat_settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.0),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.0),
        ("fDamageSkillMax", 3.0),
        ("fArmorRatingBase", 1.0),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.0),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
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

fn fixture(include_player: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x820));
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x821));
    plugin.push_top(Record::new(Tag(*b"CLAS"), 0x830));
    plugin.push_top(Record::new(Tag(*b"CLAS"), 0x831));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x840));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x841));
    for (object, race_id, class_id, female, member_faction) in [
        (0x800, 0x820, 0x830, false, 0x840),
        (0x801, 0x821, 0x831, true, 0x841),
    ] {
        let mut acbs = vec![0; 24];
        if female {
            acbs[0..4].copy_from_slice(&1_u32.to_le_bytes());
        }
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), object)
                .sub(Tag(*b"ACBS"), acbs)
                .sub(Tag(*b"RNAM"), form(race_id))
                .sub(Tag(*b"CNAM"), form(class_id))
                .sub(Tag(*b"SNAM"), faction(member_faction, 0)),
        );
    }
    if include_player {
        let mut acbs = vec![0; 24];
        acbs[0..4].copy_from_slice(&1_u32.to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0x805)
                .sub(Tag(*b"EDID"), b"Player\0".to_vec())
                .sub(Tag(*b"ACBS"), acbs)
                .sub(Tag(*b"RNAM"), form(0x821))
                .sub(Tag(*b"CNAM"), form(0x831))
                .sub(Tag(*b"SNAM"), faction(0x840, 0)),
        );
    }
    for (object, base) in [(0x900, 0x800), (0x901, 0x801)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), form(base)));
    }
    plugin.push_top(weapon(0xA00));
    add_combat_settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("PlayerConditions.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    value: &Condition,
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
        value,
    )
}

#[test]
fn ai_80_player_sex_faction_and_identity_conditions_match_ck_contracts() {
    let game = fixture(true);
    let checks = [
        (131, 0x900, 0, 0.0),
        (131, 0x900, 1, 1.0),
        (132, 0x900, 0x840, 1.0),
        (132, 0x900, 0x841, 0.0),
        (133, 0x900, 0, 1.0),
        (133, 0x901, 0, 0.0),
        (134, 0x900, 0, 0.0),
        (134, 0x901, 0, 1.0),
        (135, 0x900, 0, 0.0),
        (135, 0x901, 0, 1.0),
    ];
    for (function, subject, parameter, expected) in checks {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, parameter, 0, expected)
            ),
            Ok(true),
            "function {function}"
        );
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, parameter, 0, 1.0 - expected)
            ),
            Ok(false),
            "function {function}"
        );
    }
}

#[test]
fn ai_80_player_enemy_flag_is_read_from_the_faction_state() {
    let game = fixture(true);
    game.world_mut().faction(id(0x840)).unwrap().player_enemy = true;
    for (faction_id, expected) in [(0x840, 1.0), (0x841, 0.0)] {
        assert!(conditions::supports_function(197));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(197, faction_id, 0, expected)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                id(0x900),
                &condition(197, faction_id, 0, 1.0 - expected)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_80_player_faction_attack_records_successful_melee_and_survives_save() {
    let mut game = fixture(true);
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "health", 100.0).unwrap();
        world.set_actor_value(player, "onehanded", 100.0).unwrap();
        world.set_actor_value(id(0x900), "health", 100.0).unwrap();
        world
            .set_actor_value(id(0x900), "onehanded", 100.0)
            .unwrap();
        world.add_item(player, id(0xA00), 1);
        world.actor(player).unwrap().equipped.insert(id(0xA00));
    }
    // `GetPCFactionAttack` is false until the shared combat path commits a hit.
    assert!(conditions::supports_function(199));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(199, 0x840, 0, 0.0)),
        Ok(true)
    );
    let hit = MeleeHit {
        attacker: player,
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    };
    // The faction state changes only after the combat path commits this hit.
    assert!(game.melee_hit(hit).is_ok());
    let state = game.world_mut().faction(id(0x840)).unwrap().player_attacked;
    assert!(state);
    assert_eq!(
        evaluate(&game, id(0x900), &condition(199, 0x840, 0, 1.0)),
        Ok(true)
    );
    let saved = game.save_state().unwrap();
    let mut restored = fixture(true);
    restored.load_state(&saved).unwrap();
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(199, 0x840, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(199, 0x841, 0, 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_80_invalid_player_condition_contexts_return_diagnostics() {
    let game = fixture(true);
    assert_eq!(
        evaluate(&game, id(0x900), &condition(131, 2, 0, 0.0)),
        Err(conditions::Error::Parameter(131, 2))
    );
    for function in [132, 197, 199] {
        assert!(matches!(
            evaluate(&game, id(0x900), &condition(function, 0x820, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    for function in [133, 134, 135] {
        assert!(matches!(
            evaluate(&game, id(0xA00), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(131, 0, 1, 0.0)),
        Err(conditions::Error::Parameter(131, 1))
    );
}

#[test]
fn ai_80_player_conditions_report_missing_player_and_class_default_contracts() {
    let game = fixture(false);
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(131, 0, 0, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(132, 0x840, 0, 1.0)),
        Ok(false)
    );
    assert!(!conditions::supports_function(228));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(228, 0x830, 0, 0.0)),
        Err(conditions::Error::Function(228))
    );
}
