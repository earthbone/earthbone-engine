#![allow(clippy::unwrap_used)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn form(object: u32) -> Vec<u8> {
    object.to_le_bytes().to_vec()
}

fn faction(id: u32, rank: i8) -> Vec<u8> {
    [id.to_le_bytes().as_slice(), &[rank as u8]].concat()
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

fn fixture() -> Game {
    fixture_with_player(true)
}

fn fixture_with_player(include_player: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x820));
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x821));
    plugin.push_top(Record::new(Tag(*b"CLAS"), 0x830));
    plugin.push_top(Record::new(Tag(*b"CLAS"), 0x831));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x840));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x841));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x850));
    for (object, race, class, female, factions) in [
        (0x800, 0x820, 0x830, false, vec![(0x840, 2)]),
        (0x801, 0x820, 0x830, false, vec![(0x840, -1)]),
        (0x802, 0x821, 0x831, true, vec![(0x841, 0)]),
        (0x803, 0x820, 0x831, true, vec![]),
    ] {
        let mut acbs = vec![0; 24];
        if female {
            acbs[0..4].copy_from_slice(&1_u32.to_le_bytes());
        }
        let mut npc = Record::new(Tag(*b"NPC_"), object)
            .sub(Tag(*b"ACBS"), acbs)
            .sub(Tag(*b"RNAM"), form(race))
            .sub(Tag(*b"CNAM"), form(class));
        for (faction_id, rank) in factions {
            npc = npc.sub(Tag(*b"SNAM"), faction(faction_id, rank));
        }
        plugin.push_top(npc);
    }
    if include_player {
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0x805)
                .sub(Tag(*b"EDID"), b"Player\0".to_vec())
                .sub(Tag(*b"ACBS"), vec![0; 24])
                .sub(Tag(*b"RNAM"), form(0x820))
                .sub(Tag(*b"CNAM"), form(0x830))
                .sub(Tag(*b"SNAM"), faction(0x840, 0)),
        );
    }
    for (object, base) in [
        (0x900, 0x800),
        (0x901, 0x801),
        (0x902, 0x802),
        (0x903, 0x803),
    ] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), form(base)));
    }
    let mut order = LoadOrder::new();
    order.add("Relationships.esm", plugin).unwrap();
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
        },
        c,
    )
}

#[test]
fn ai_79_relationship_comparisons_match_shared_factions_races_and_sexes() {
    let game = fixture();
    for (function, subject, other, expected) in [
        (42, 0x900, 0x901, 1.0),
        (42, 0x900, 0x902, 0.0),
        (43, 0x900, 0x901, 1.0),
        (43, 0x900, 0x902, 0.0),
        (44, 0x900, 0x901, 1.0),
        (44, 0x900, 0x902, 0.0),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(subject), &condition(function, other, 0, expected)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, other, 0, 1.0 - expected)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_79_faction_rank_difference_is_subject_rank_minus_parameter_actor_rank() {
    let game = fixture();
    assert!(conditions::supports_function(60));
    for (subject, other, difference) in [(0x900, 0x901, 3.0), (0x901, 0x900, -3.0)] {
        assert_eq!(
            evaluate(&game, id(subject), &condition(60, 0x840, other, difference)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(60, 0x840, other, -difference)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_79_class_and_player_identity_conditions_read_actor_bases() {
    let game = fixture();
    for (function, subject, parameter, expected) in [
        (68, 0x900, 0x830, 1.0),
        (68, 0x900, 0x831, 0.0),
        (129, 0x900, 0x830, 1.0),
        (129, 0x900, 0x831, 0.0),
        (130, 0x900, 0x820, 1.0),
        (130, 0x900, 0x821, 0.0),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, parameter, 0, expected)
            ),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, parameter, 0, 1.0 - expected)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_79_invalid_actor_and_parameter_contexts_return_form_diagnostics() {
    let game = fixture();
    for function in [42, 43, 44] {
        assert_eq!(
            evaluate(&game, id(0x850), &condition(function, 0x901, 0, 0.0)),
            Err(conditions::Error::Form(id(0x850)))
        );
        assert!(matches!(
            evaluate(&game, id(0x900), &condition(function, 0x850, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(60, 0x850, 0x901, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(60, 0x840, 0x850, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    for (function, bad_parameter) in [(68, 0x820), (129, 0x820), (130, 0x830)] {
        assert!(matches!(
            evaluate(
                &game,
                id(0x900),
                &condition(function, bad_parameter, 0, 0.0)
            ),
            Err(conditions::Error::Form(_))
        ));
        if function == 68 {
            assert_eq!(
                evaluate(&game, id(0x850), &condition(function, 0x830, 0, 0.0)),
                Err(conditions::Error::Form(id(0x850)))
            );
        }
    }
}

#[test]
fn ai_79_player_conditions_diagnose_missing_player_base_context() {
    let game = fixture_with_player(false);
    for (function, parameter) in [(129, 0x830), (130, 0x820)] {
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, parameter, 0, 0.0)),
            Err(conditions::Error::Form(id(0x14)))
        );
    }
}

#[test]
fn ai_79_get_is_creature_reports_missing_usable_contract() {
    let game = fixture();
    assert!(!conditions::supports_function(64));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(64, 0, 0, 0.0)),
        Err(conditions::Error::Function(64))
    );
}
