#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

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

fn relationship(parent: u32, child: u32, rank: u16, association: u32) -> Vec<u8> {
    [
        parent.to_le_bytes().as_slice(),
        child.to_le_bytes().as_slice(),
        rank.to_le_bytes().as_slice(),
        &[0, 0],
        association.to_le_bytes().as_slice(),
    ]
    .concat()
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
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    for (object, base) in [(0x900, 0x800), (0x901, 0x801), (0x902, 0x802)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), form(base)));
    }
    plugin.push_top(Record::new(Tag(*b"ASTP"), 0x840).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ASTP"), 0x841).sub(Tag(*b"DATA"), 1_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"RELA"), 0x850).sub(Tag(*b"DATA"), relationship(0x800, 0x801, 1, 0x840)),
    );
    plugin.push_top(
        Record::new(Tag(*b"RELA"), 0x851).sub(Tag(*b"DATA"), relationship(0x800, 0x802, 7, 0x841)),
    );
    plugin.push_top(
        Record::new(Tag(*b"RELA"), 0x852).sub(Tag(*b"DATA"), relationship(0x801, 0x802, 3, 0x840)),
    );
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
fn ai_82_highest_and_lowest_relationship_ranks_use_authored_relationships() {
    let game = fixture();
    for (function, subject, parameter, rank) in [(615, 0x900, 0, 3.0), (616, 0x900, 0, -3.0)] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(subject), &condition(function, parameter, 0, rank)),
            Ok(true),
            "function {function}"
        );
        assert_eq!(
            evaluate(
                &game,
                id(subject),
                &condition(function, parameter, 0, rank + 1.0)
            ),
            Ok(false)
        );
    }
}

#[test]
fn ai_82_teammate_association_and_family_any_match_actor_state_and_records() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().teammate = true;
    for (function, subject, parameter, expected) in [
        (453, 0x900, 0, 1.0),
        (453, 0x901, 0, 0.0),
        (617, 0x900, 0x840, 1.0),
        (617, 0x900, 0x841, 1.0),
        (617, 0x901, 0x841, 0.0),
        (618, 0x900, 0, 1.0),
        (618, 0x901, 0, 0.0),
    ] {
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
            Ok(false)
        );
    }
}

#[test]
fn ai_82_invalid_actor_and_parameters_return_form_or_parameter_diagnostics() {
    let game = fixture();
    for function in [453, 615, 616, 617, 618] {
        assert!(matches!(
            evaluate(&game, id(0x850), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(_))
        ));
    }
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(617, 0x800, 0, 0.0)),
        Err(conditions::Error::Form(_))
    ));
    for (function, first, second) in [
        (453, 1, 0),
        (615, 0, 1),
        (616, 0, 1),
        (617, 0x840, 1),
        (618, 0, 1),
    ] {
        assert!(matches!(
            evaluate(&game, id(0x900), &condition(function, first, second, 0.0)),
            Err(conditions::Error::Parameter(_, _))
        ));
    }
}

#[test]
fn ai_82_teammate_count_and_exit_instant_remain_unsupported_with_diagnostics() {
    let game = fixture();
    for function in [449, 454, 637] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn authored_relationship_extrema_ignore_runtime_relationship_overrides() {
    let game = fixture();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .relationships
        .insert(id(0x901), 4);
    assert_eq!(
        evaluate(&game, id(0x900), &condition(615, 0, 0, 3.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(615, 0, 0, 4.0)),
        Ok(false)
    );
}
