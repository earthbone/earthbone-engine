#![allow(clippy::unwrap_used)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
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
    let mut plugin = Plugin::new(FileHeader::default());
    let mut playable = vec![0; 124];
    playable[32..36].copy_from_slice(&1_u32.to_le_bytes());
    let mut child = vec![0; 124];
    child[32..36].copy_from_slice(&4_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x820).sub(Tag(*b"DATA"), playable));
    plugin.push_top(Record::new(Tag(*b"RACE"), 0x821).sub(Tag(*b"DATA"), child));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"RNAM"), 0x820_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801).sub(Tag(*b"RNAM"), 0x821_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    for (object, base) in [(0x900, 0x800), (0x901, 0x801), (0x902, 0x802)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), base_u32(base)));
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x910));
    plugin.push_top(Record::new(Tag(*b"ASTP"), 0x840).sub(Tag(*b"DATA"), 1_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ASTP"), 0x841).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"RELA"), 0x850).sub(
            Tag(*b"DATA"),
            [
                0x800_u32.to_le_bytes().as_slice(),
                0x801_u32.to_le_bytes().as_slice(),
                0_u16.to_le_bytes().as_slice(),
                &[0, 0],
                0x840_u32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
    );
    for (object, other, reaction) in [(0x860_u32, 0x861_u32, 1_i32), (0x861_u32, 0x860_u32, 3_i32)]
    {
        plugin.push_top(
            Record::new(Tag(*b"FACT"), object).sub(
                Tag(*b"XNAM"),
                [
                    other.to_le_bytes().as_slice(),
                    &[0; 4],
                    reaction.to_le_bytes().as_slice(),
                ]
                .concat(),
            ),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
    Game::new(order)
}

fn base_u32(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
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
fn ai_81_playable_race_and_child_flags_match_positive_and_negative_races() {
    let game = fixture();
    assert_eq!(
        game.world_mut().actor_base(id(0x800)).unwrap().race,
        Some(id(0x820))
    );
    assert_eq!(
        game.world_mut().actor_base(id(0x801)).unwrap().race,
        Some(id(0x821))
    );
    assert_eq!(
        game.world_mut()
            .record(id(0x820))
            .unwrap()
            .find(Tag(*b"DATA"))
            .unwrap()[32..36],
        1_u32.to_le_bytes()
    );
    for (function, subject, expected) in [
        (254, id(0x900), 1.0),
        (254, id(0x901), 0.0),
        (365, id(0x901), 1.0),
        (365, id(0x900), 0.0),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, subject, &condition(function, 0, 0, expected)),
            Ok(true),
            "function {function} for {subject}"
        );
        assert_eq!(
            evaluate(&game, subject, &condition(function, 0, 0, 1.0 - expected)),
            Ok(false)
        );
    }
}

#[test]
fn ai_81_association_family_and_parent_relations_follow_rela_records() {
    let game = fixture();
    let checks = [
        (258, id(0x900), 0x901, 0x840, 1.0),
        (258, id(0x900), 0x902, 0x840, 0.0),
        (258, id(0x900), 0x901, 0x841, 0.0),
        (259, id(0x900), 0, 0, 1.0),
        (259, id(0x902), 0, 0, 0.0),
        (259, id(0x900), 0x901, 0, 1.0),
        (259, id(0x900), 0x902, 0, 0.0),
        (261, id(0x900), 0x901, 0, 1.0),
        (261, id(0x901), 0x900, 0, 0.0),
    ];
    for (function, subject, first, second, expected) in checks {
        let c = condition(function, first, second, expected);
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, subject, &c),
            Ok(true),
            "{function}, {first:#x}"
        );
        assert_eq!(
            evaluate(
                &game,
                subject,
                &condition(function, first, second, 1.0 - expected)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_81_faction_combat_reaction_uses_ordered_xnam_relation() {
    let game = fixture();
    for (first, second, reaction) in [
        (0x860, 0x861, 1.0),
        (0x861, 0x860, 3.0),
        (0x860, 0x860, 0.0),
    ] {
        let c = condition(410, first, second, reaction);
        assert!(conditions::supports_function(410));
        assert_eq!(evaluate(&game, id(0x900), &c), Ok(true));
        assert_eq!(
            evaluate(
                &game,
                id(0x900),
                &condition(410, first, second, reaction + 1.0)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_81_invalid_contexts_and_undocumented_functions_are_diagnostic() {
    let game = fixture();
    for function in [254, 258, 259, 261, 365] {
        assert_eq!(
            evaluate(&game, id(0x910), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(0x910)))
        );
    }
    assert_eq!(
        evaluate(&game, id(0x900), &condition(254, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(254, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(365, 1, 0, 0.0)),
        Err(conditions::Error::Parameter(365, 1))
    );
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(258, 0xfff, 0x840, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(258, 0x901, 0x910, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(259, 0xfff, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(261, 0, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(410, 0x800, 0x861, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    for function in [229, 437] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn authored_parent_relationship_is_not_reversed_by_repeated_reads() {
    let game = fixture();
    let parent = id(0x900);
    let child = id(0x901);
    let forward = condition(261, 0x901, 0, 1.0);
    let reverse = condition(261, 0x900, 0, 1.0);
    for _ in 0..3 {
        assert_eq!(evaluate(&game, parent, &forward), Ok(true));
        assert_eq!(evaluate(&game, child, &reverse), Ok(false));
    }
}
