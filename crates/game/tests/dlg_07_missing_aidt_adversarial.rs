// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass on DLG-07, missing-record angle.
//!
//! `conditions.rs`'s own comment on GetIntimidateSuccess (function 655, line
//! 472) states the formula compares "the player's Speechcraft against the
//! subject's Confidence (AIDT 0 Cowardly to 4 Foolhardy)". That Confidence
//! value is wired from the NPC's AIDT subrecord into the "confidence" actor
//! value at actor construction time
//! (`crates/game/src/world/actors.rs`, the `ai_source.ai_data` block: `("confidence",
//! data.confidence)`, fed by `AiData::from_bytes` in `crates/game/src/npc.rs`).
//!
//! `npc.rs` parses AIDT as `Option<AiData>` (`b"AIDT" => n.ai_data =
//! Some(AiData::from_bytes(d)?)`), so a record with no AIDT subrecord at all
//! leaves `ai_data` as `None`. `world/actors.rs` only writes the six AI
//! actor values `if let Some(data) = ai_source.ai_data`, so a record missing
//! AIDT gets no confidence entry, and `Values::get` (`actor_values.rs`)
//! returns `Components::default()`, base 0.0, for any name it has never
//! seen. Confidence silently reads 0.0, the "Cowardly" end of the documented
//! 0 to 4 range, for a record that supplied no AI data whatsoever, and the
//! esm parser raises no error for the missing subrecord (`AIDT` is matched
//! only if present, never required).
//!
//! CTDA records reference NPC_ records built by third-party plugins, which
//! this engine must accept even when incomplete
//! (docs/game.md's DLG-07 section: the functions must resolve against
//! arbitrary load-order data, not only a fully authored vanilla record). A
//! record missing AIDT is exactly the "malformed or incomplete load order
//! record" case the done check's live-data angle covers. Treating an absent
//! Confidence as the most intimidatable value instead of erroring produces a
//! GetIntimidateSuccess result no CK author asked for: any player with
//! nonnegative Speechcraft succeeds against a record that never said
//! anything about how confident its subject is.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

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

/// A speaker's `NPC_` record with no `AIDT` subrecord (legal input, the esm
/// parser never requires it) leaves the subject's "confidence" actor value
/// at its unset default of 0.0, the "Cowardly" end of the documented 0 to 4
/// AIDT range. With the player's Speechcraft left at its own unset default
/// of 0.0 too, GetIntimidateSuccess (655) computes `0.0 >= 0.0 * 25.0`, true,
/// so a player who has trained no Speechcraft at all "succeeds" at
/// intimidating a subject the record never described as cowardly, timid, or
/// anything else. A record silently missing its AI data must not read as
/// the easiest possible target to intimidate.
#[test]
fn dlg_07_intimidate_success_treats_a_speaker_missing_aidt_as_maximally_cowardly_adversarial() {
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

    // Player Speechcraft is also left at its unset default (0.0), the
    // weakest possible persuader. Per the documented formula
    // (speechcraft >= confidence * 25.0), this should fail against any
    // subject whose Confidence is not itself the minimum, 0 (Cowardly).
    // A record that never supplied AI data has not claimed to be Cowardly.
    let succeeds = condition(655, 1.0);
    let result = evaluate(&game, subject, &succeeds);
    assert_eq!(
        result,
        Ok(false),
        "GetIntimidateSuccess (655) must not succeed for an unskilled \
         player (Speechcraft 0.0) against a speaker whose AIDT subrecord is \
         entirely absent; got {result:?} because the missing record reads \
         as Confidence 0.0 (Cowardly), conditions.rs line 472 to 488, \
         instead of an unknown value that should fail the check closed"
    );
}

/// Same gap from the comparative side: two speakers differ only in whether
/// their `NPC_` record carries an `AIDT` subrecord. One states Confidence 4
/// (Foolhardy, the hardest to intimidate) explicitly. The other has no AIDT
/// at all. GetIntimidateSuccess must not treat the two identically, since
/// nothing in the second record claims Foolhardy is wrong, but nothing
/// claims Cowardly is right either. A fixed, moderate Speechcraft that
/// correctly fails against the declared Foolhardy speaker exposes the
/// silent default by succeeding against the undeclared one instead of
/// erroring or matching the same failure.
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
        Ok(false),
        "sanity: Speechcraft 50 against declared Confidence 4 (needs 100) should fail"
    );

    let missing_result = evaluate(&game, id(0x901), &check);
    assert_eq!(
        missing_result,
        Ok(false),
        "GetIntimidateSuccess (655) succeeded ({missing_result:?}) against a \
         speaker whose AIDT subrecord is entirely missing, at the same \
         Speechcraft (50) that correctly fails against a speaker who \
         declared Confidence 4 (Foolhardy). A record silent on Confidence \
         must not become the easiest one in the load order to intimidate, \
         conditions.rs line 472 to 488"
    );
}
