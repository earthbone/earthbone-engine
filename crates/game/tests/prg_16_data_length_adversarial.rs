// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass on PRG-16 (non-default PERK purchase DATA semantics).
//!
//! PRG-16's writer summary and `docs/game.md`, "Ordinary perk purchases
//! (PRG-02)", both state that DATA's Trait, Level, Num Ranks and Hidden
//! bytes carry no purchase meaning and cite wrye-bash's `MelPerkData` in
//! `Mopy/bash/brec/common_subrecords.py` as one of the sources read for that
//! claim. That same class is declared as
//! `class MelPerkData(MelTruncatedStruct)` with
//! `old_versions={'4B', '3B'}`, meaning the shared Bethesda-record reader
//! wrye-bash uses for Skyrim (`Mopy/bash/game/skyrim/records.py`, `MrePerk`)
//! explicitly accepts a PERK DATA subrecord that is 3 or 4 bytes long, not
//! only 5, padding the missing trailing fields with defaults instead of
//! rejecting the record. `crates/game/src/perks.rs`'s `Perk::read` still
//! hard-rejects any DATA subrecord whose length is not exactly 5
//! (`sub.data.len() != 5` returns `Error::Perk`), so a perk record in this
//! historically real, tool-documented shorter encoding is currently
//! reported as "missing, deleted or malformed" even though the bytes that
//! are present are perfectly readable and PRG-16 itself established that
//! none of the trailing fields affect eligibility to purchase.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{Error, Purchase},
    Game,
};
use std::collections::BTreeSet;

const FIRST: u32 = 0x900;
const SECOND: u32 = 0x901;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Perks.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        let player = world.player();
        world.actor(player).unwrap();
        world.perk_points = 5;
    }
    game
}

fn state(game: &Game) -> (i32, BTreeSet<GlobalFormId>) {
    let mut world = game.world_mut();
    let player = world.player();
    (
        world.perk_points,
        world.actor(player).unwrap().perks.clone(),
    )
}

/// A 4-byte DATA (Trait, Level, Num Ranks, Playable, Hidden defaulted) is a
/// documented old-format encoding, not a malformed record. Source: wrye-bash
/// `MelPerkData(MelTruncatedStruct)`, `old_versions={'4B', '3B'}`, used
/// unmodified for Skyrim's `MrePerk`. A conditionless, unchained perk in
/// this encoding with Playable explicit and true must purchase normally.
#[test]
fn four_byte_legacy_data_purchases_instead_of_reporting_malformed() {
    let record = Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 1]);
    let mut game = game(vec![record]);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Ok(Purchase {
            perk: id(FIRST),
            rank: 1
        }),
        "a 4-byte DATA subrecord is the documented wrye-bash old_versions \
         encoding and must not be treated as a malformed record"
    );
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
}

/// A 3-byte DATA (only Trait, Level, Num Ranks present) is the same
/// documented old-format encoding one step further truncated. Source: same
/// wrye-bash `MelPerkData` `old_versions` entry. Whatever the resolved
/// Playable default, the record itself is well-formed and readable, so it
/// must not be reported as `Error::Perk` (missing, deleted or malformed).
#[test]
fn three_byte_legacy_data_is_not_reported_as_malformed() {
    let record = Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1]);
    let mut game = game(vec![record]);
    let before = state(&game);
    let result = game.purchase_perk(id(FIRST));
    assert_ne!(
        result,
        Err(Error::Perk(id(FIRST))),
        "a 3-byte DATA subrecord is the documented wrye-bash old_versions \
         encoding, it must not be rejected as missing, deleted or malformed \
         (got {result:?})"
    );
    // Either the purchase went through or it failed for a reason other than
    // being malformed, either way a rejected purchase must not spend a point.
    if result.is_err() {
        assert_eq!(state(&game), before);
    }
}

/// `rank_chain` reads every record in a chain up front, before the caller's
/// requested rank is even located. A legacy-length DATA on a later, still
/// unpurchased rank must not block purchase of an earlier rank whose own
/// record is fully well-formed: only the record actually being evaluated
/// for playability and prerequisites should matter to that purchase.
/// Source: same wrye-bash `MelPerkData` `old_versions` entry, applied to the
/// two-record NNAM chain shape documented in `docs/game.md`.
#[test]
fn legacy_data_later_in_a_chain_does_not_block_purchase_of_an_earlier_rank() {
    let root = Record::new(Tag(*b"PERK"), FIRST)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"NNAM"), SECOND.to_le_bytes());
    let second = Record::new(Tag(*b"PERK"), SECOND).sub(Tag(*b"DATA"), [0, 0, 1, 1]);
    let mut game = game(vec![root, second]);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Ok(Purchase {
            perk: id(FIRST),
            rank: 1
        }),
        "rank 1's own record is fully well-formed, a legacy-length DATA on \
         the unpurchased rank 2 later in the same chain must not poison it"
    );
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
}

/// Same as above with the more extreme 3-byte legacy encoding on the later
/// rank, to bound how much of the record the length check may legitimately
/// still require before it stops being a length check on the requested
/// rank and starts being a length check on the whole chain.
#[test]
fn three_byte_legacy_data_later_in_a_chain_does_not_block_purchase_of_an_earlier_rank() {
    let root = Record::new(Tag(*b"PERK"), FIRST)
        .sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
        .sub(Tag(*b"NNAM"), SECOND.to_le_bytes());
    let second = Record::new(Tag(*b"PERK"), SECOND).sub(Tag(*b"DATA"), [0, 0, 1]);
    let mut game = game(vec![root, second]);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Ok(Purchase {
            perk: id(FIRST),
            rank: 1
        }),
        "rank 1's own record is fully well-formed, a 3-byte legacy DATA on \
         the unpurchased rank 2 later in the same chain must not poison it"
    );
    assert_eq!(state(&game), (4, BTreeSet::from([id(FIRST)])));
}
