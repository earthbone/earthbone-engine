// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-73 reference state CTDA contracts.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

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
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x802));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    for (object, base, owner) in [
        (0x900_u32, 0x800_u32, None),
        (0x901, 0x801, None),
        (0x902_u32, 0x801_u32, Some(0x801_u32)),
    ] {
        let mut record = Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), base.to_le_bytes());
        if let Some(owner) = owner {
            record = record.sub(Tag(*b"XOWN"), owner.to_le_bytes());
        }
        plugin.push_top(record);
    }
    for (object, owner) in [
        (0x910_u32, Some(0x801_u32)),
        (0x911_u32, Some(0x802_u32)),
        (0x912_u32, Some(0x800_u32)),
    ] {
        let mut record =
            Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes());
        if let Some(owner) = owner {
            record = record.sub(Tag(*b"XOWN"), owner.to_le_bytes());
        }
        plugin.push_top(record);
    }
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x930).sub(Tag(*b"DATA"), vec![1]));
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x931).sub(Tag(*b"DATA"), vec![0]));
    let mut order = LoadOrder::new();
    order.add("Conditions.esm", plugin).unwrap();
    let game = Game::new(order);
    for reference in [0x910, 0x911, 0x912] {
        game.world_mut().reference(id(reference)).unwrap();
    }
    game.world_mut().reference(id(0x910)).unwrap().cell = Some(id(0x930));
    game.world_mut().reference(id(0x911)).unwrap().cell = Some(id(0x931));
    game
}

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    condition: &Condition,
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
        condition,
    )
}

#[test]
fn ai_73_ghost_and_unconscious_read_actor_state() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().ghost = true;
    game.world_mut().actor(id(0x901)).unwrap().unconscious = true;

    for (function, subject, positive) in [
        (237, id(0x900), true),
        (237, id(0x901), false),
        (242, id(0x901), true),
        (242, id(0x900), false),
    ] {
        assert!(conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, subject, &condition(function, 0, 0, 1.0)),
            Ok(positive)
        );
        assert_eq!(
            evaluate(&game, subject, &condition(function, 0, 0, 0.0)),
            Ok(!positive)
        );
    }
}

#[test]
fn ai_73_owner_matches_explicit_owner_and_null_means_player() {
    let game = fixture();
    let checks = [
        (id(0x910), 0x801, true),
        (id(0x910), 0x802, false),
        (id(0x911), 0x802, true),
        (id(0x912), 0, true),
        (id(0x910), 0, false),
    ];
    for (subject, owner, positive) in checks {
        assert_eq!(
            evaluate(&game, subject, &condition(278, owner, 0, 1.0)),
            Ok(positive),
            "subject {subject}, owner {owner:#x}"
        );
        assert_eq!(
            evaluate(&game, subject, &condition(278, owner, 0, 0.0)),
            Ok(!positive)
        );
    }
    assert_eq!(
        evaluate(&game, id(0x910), &condition(278, 0x930, 0, 1.0)),
        Err(conditions::Error::Form(id(0x930)))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(278, 0x801, 1, 1.0)),
        Err(conditions::Error::Parameter(278, 1))
    );
}

#[test]
fn ai_73_interior_uses_the_subjects_parent_cell() {
    let game = fixture();
    for (subject, positive) in [(id(0x910), true), (id(0x911), false), (id(0x912), false)] {
        assert_eq!(
            evaluate(&game, subject, &condition(300, 0, 0, 1.0)),
            Ok(positive)
        );
        assert_eq!(
            evaluate(&game, subject, &condition(300, 0, 0, 0.0)),
            Ok(!positive)
        );
    }
}

#[test]
fn ai_73_invalid_contexts_and_undocumented_functions_are_diagnostic() {
    let game = fixture();
    for function in [237, 242] {
        assert_eq!(
            evaluate(&game, id(0x910), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(0x910)))
        );
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, 1, 0, 0.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
    }
    for function in [278, 300] {
        assert_eq!(
            evaluate(&game, id(0x800), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Form(id(0x800)))
        );
    }
    for function in [262, 274, 285, 305] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(0x910), &condition(function, 0, 0, 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}
