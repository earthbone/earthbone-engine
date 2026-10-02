// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-07 missing-record checks for GetIntimidateSuccess (655).
//! Missing AIDT must report a state-contract error instead of reading unset
//! Confidence as Cowardly. Authored Confidence still lacks the perk formula
//! named by local CK `WillIntimidateSucceed_-_Actor.wikitext`.

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
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

/// Eight-byte AIDT layout from `combat_ai.rs`: aggression, confidence,
/// energy, morality, mood, assistance, two unused bytes, then three u32
/// radii. Only `confidence` varies here.
fn ai_data(confidence: u8) -> Vec<u8> {
    let mut data = vec![0u8, confidence, 70, 1, 3, 2, 1, 0];
    for radius in [500_u32, 300, 100] {
        data.extend_from_slice(&radius.to_le_bytes());
    }
    data
}

/// Missing AIDT is distinct from an authored Confidence value of zero.
#[test]
fn dlg_07_intimidate_success_reports_missing_aidt_contract_adversarial() {
    let mut plugin = Plugin::new(FileHeader::default());
    // Speaker NPC_ record has no AIDT subrecord at all, matching a plugin
    // that never authored AI data for this actor rather than one that
    // deliberately set Confidence to 0 (Cowardly).
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("MissingAidt.esm", plugin).unwrap();
    let game = Game::new(order);

    let subject = id(0x900);
    // Confirm the missing AIDT really does leave confidence at the default,
    // rather than some other fallback the source comment did not predict.
    assert_eq!(
        game.world_mut().actor_value(subject, "confidence"),
        0.0,
        "sanity: a speaker record with no AIDT subrecord should read \
         confidence 0.0 (Values::get's unset default), not error or inherit \
         a nonzero value from somewhere else"
    );

    let succeeds = condition(655, 1.0);
    let result = evaluate(&game, subject, &succeeds);
    assert_eq!(
        result,
        Err(conditions::Error::StateContract(655)),
        "missing AIDT must report an unknown Confidence contract"
    );
}

/// Authored and missing Confidence produce distinct diagnostics.
#[test]
fn dlg_07_intimidate_success_differs_between_declared_and_missing_confidence_adversarial() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801).sub(Tag(*b"AIDT"), ai_data(4)));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("MissingAidtCompare.esm", plugin).unwrap();
    let game = Game::new(order);

    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 50.0)
        .unwrap();

    let declared_confidence = game.world_mut().actor_value(id(0x900), "confidence");
    assert_eq!(
        declared_confidence, 4.0,
        "sanity: the first speaker's explicit AIDT should read Confidence 4.0"
    );
    let missing_confidence = game.world_mut().actor_value(id(0x901), "confidence");
    assert_eq!(
        missing_confidence, 0.0,
        "sanity: the second speaker's absent AIDT should read the unset default"
    );

    let check = condition(655, 1.0);
    let declared_result = evaluate(&game, id(0x900), &check);
    assert_eq!(
        declared_result,
        Err(conditions::Error::Unavailable(655)),
        "authored Confidence does not supply the missing perk formula"
    );

    let missing_result = evaluate(&game, id(0x901), &check);
    assert_eq!(
        missing_result,
        Err(conditions::Error::StateContract(655)),
        "missing Confidence must retain its distinct state-contract error"
    );
}
