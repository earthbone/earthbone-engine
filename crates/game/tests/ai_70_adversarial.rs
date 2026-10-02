// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-70 reference state condition functions.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{
    Game, conditions,
    detection::{DetectionLevel, Motion, Observation},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x900_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x911).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"GMST"), 0x1000)
            .zsub(Tag(*b"EDID"), "fSneakBaseValue")
            .sub(Tag(*b"DATA"), 0.0_f32.to_le_bytes()),
    );
    let floats: [(&str, f32); 23] = [
        ("fSneakMaxDistance", 100.0),
        ("fSneakDistanceAttenuationExponent", 1.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 0.0),
        ("fSneakEquippedWeightMult", 1.0),
        ("fSneakRunningMult", 1.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.0),
        ("fSneakLightRunMult", 0.0),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 1.0),
        ("fSneakStealthBoyMult", 1.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 100.0),
        ("fSneakAlertMod", 0.0),
        ("fSneakCombatMod", 0.0),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 5.0),
        ("fCombatStealthPointRegenLostWaitTime", 5.0),
    ];
    for (index, (name, value)) in floats.into_iter().enumerate() {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1001 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    for (object, name, value) in [
        (0x1100, "iCombatStealthPointDetectionThreshold", 50_i32),
        (0x1101, "iCombatStealthPointSneakDetectionThreshold", 70_i32),
    ] {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), object)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("AI70.esm", plugin).unwrap();
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
fn ai_70_reference_queries_match_scale_disabled_and_alarm_state() {
    let game = fixture();
    game.world_mut().reference(id(0x910)).unwrap().scale = 1.5;
    game.world_mut().reference(id(0x911)).unwrap().enabled = false;
    game.world_mut().actor(id(0x900)).unwrap().alerted = true;
    for (function, subject, expected) in [
        (24, id(0x910), 1.5),
        (35, id(0x911), 1.0),
        (61, id(0x900), 1.0),
    ] {
        let yes = condition(function, 0, expected);
        let no = condition(function, 0, expected + 1.0);
        assert!(conditions::supports_function(function));
        assert_eq!(evaluate(&game, subject, &yes), Ok(true), "{function}");
        assert_eq!(evaluate(&game, subject, &no), Ok(false), "{function}");
    }
    assert_eq!(
        evaluate(&game, id(0xfff), &condition(35, 0, 0.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );
    assert_eq!(
        evaluate(&game, id(0xfff), &condition(24, 0, 0.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(61, 0, 0.0)),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_70_detected_checks_observer_awareness_and_actor_context() {
    let game = fixture();
    let observation = Observation {
        observer: id(0x901),
        target: id(0x900),
        distance: 0.0,
        line_of_sight: true,
        light_level: 100.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    };
    game.world_mut()
        .evaluate_detection(&[observation], 0.0)
        .unwrap();
    assert_eq!(
        game.world().awareness(id(0x901), id(0x900)).unwrap().level,
        DetectionLevel::Combat
    );
    let detected = condition(45, 0x901, 1.0);
    let undetected = condition(45, 0x901, 0.0);
    assert_eq!(evaluate(&game, id(0x900), &detected), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &undetected), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(45, 0x900, 0.0)),
        Ok(true)
    );
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(45, 0x910, 0.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_70_random_percent_compares_a_bounded_roll() {
    let game = fixture();
    let mut positive = condition(77, 0, -1.0);
    positive.flags = 2 << 5;
    let mut negative = condition(77, 0, 100.0);
    negative.flags = 2 << 5;
    assert_eq!(evaluate(&game, id(0x900), &positive), Ok(true));
    assert_eq!(evaluate(&game, id(0x900), &negative), Ok(false));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(77, 1, 100.0)),
        Err(conditions::Error::Parameter(77, 1))
    );
}

#[test]
fn ai_70_unavailable_contracts_report_explicit_diagnostics() {
    let game = fixture();
    for function in [27, 36, 39] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 1.0)),
            Err(conditions::Error::UnavailableContractReason(
                function,
                "the local CK corpus does not define a runtime contract",
            ))
        );
    }
}
