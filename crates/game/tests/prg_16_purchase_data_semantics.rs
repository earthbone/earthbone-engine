// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Planned check for PRG-16: authored (non-default) Trait, Level, Num Ranks
//! and Hidden bytes in PERK DATA follow the purchase rules verified in
//! `docs/game.md`, "Ordinary perk purchases (PRG-02)": Trait and Level carry
//! no purchase meaning, the granted rank number comes from the NNAM chain's
//! own length rather than the Num Ranks byte, and Hidden does not gate or
//! change a purchase since this API has no UI layer for it to affect.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    perks::{Error, Purchase},
    Game,
};
use std::collections::BTreeSet;

const FIRST: u32 = 0xA00;
const SECOND: u32 = 0xA01;

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

/// Trait and Level are documented as carrying no purchase meaning: an
/// authored, non-default pair must purchase identically to the all-zero
/// baseline, same rank, same resulting state.
#[test]
fn prg_16_authored_trait_and_level_do_not_gate_purchase() {
    let mut baseline = game(vec![
        Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
    ]);
    let mut authored = game(vec![
        Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [200, 90, 1, 1, 0])
    ]);
    let expected = Ok(Purchase {
        perk: id(FIRST),
        rank: 1,
    });
    assert_eq!(baseline.purchase_perk(id(FIRST)), expected);
    assert_eq!(authored.purchase_perk(id(FIRST)), expected);
    assert_eq!(state(&baseline), state(&authored));
    assert_eq!(state(&authored), (4, BTreeSet::from([id(FIRST)])));
}

/// Num Ranks is a Creation Kit display field, not a purchase input: the
/// granted rank number must come from walking the NNAM chain, so an authored
/// byte that disagrees with the chain's real length (5 claimed, 2 actual)
/// must not change which rank each purchase grants, nor let a third
/// purchase through once the two-record chain is exhausted.
#[test]
fn prg_16_num_ranks_byte_does_not_override_the_chain_length() {
    let root = Record::new(Tag(*b"PERK"), FIRST)
        .sub(Tag(*b"DATA"), [0, 0, 5, 1, 0])
        .sub(Tag(*b"NNAM"), SECOND.to_le_bytes());
    let second = Record::new(Tag(*b"PERK"), SECOND).sub(Tag(*b"DATA"), [0, 0, 5, 1, 0]);
    let mut game = game(vec![root, second]);
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Ok(Purchase {
            perk: id(FIRST),
            rank: 1
        })
    );
    assert_eq!(
        game.purchase_perk(id(FIRST)),
        Ok(Purchase {
            perk: id(SECOND),
            rank: 2
        })
    );
    assert_eq!(game.purchase_perk(id(FIRST)), Err(Error::Complete));
    assert_eq!(state(&game), (3, BTreeSet::from([id(FIRST), id(SECOND)])));
}

/// Hidden only ever affected CK or SKSE UI listing, which this API has no
/// layer for, so a Hidden perk must purchase identically to a shown one.
#[test]
fn prg_16_hidden_value_does_not_gate_or_alter_purchase() {
    let mut shown = game(vec![
        Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 1, 0])
    ]);
    let mut hidden = game(vec![
        Record::new(Tag(*b"PERK"), FIRST).sub(Tag(*b"DATA"), [0, 0, 1, 1, 1])
    ]);
    let expected = Ok(Purchase {
        perk: id(FIRST),
        rank: 1,
    });
    assert_eq!(shown.purchase_perk(id(FIRST)), expected);
    assert_eq!(hidden.purchase_perk(id(FIRST)), expected);
    assert_eq!(state(&shown), state(&hidden));
    assert_eq!(state(&hidden), (4, BTreeSet::from([id(FIRST)])));
}
