// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Planned check for PRG-06: "A witnessed crime changes only the owning
//! faction bounty and persists the known witness state."
//!
//! `World::record_witnessed_crime` (`crates/game/src/world/crime.rs`) prices
//! a crime against the perpetrator's own crime faction only (`Actor.GetCrimeFaction`,
//! ck-wiki) and records the witness's knowledge in `ActorState::witnessed_crimes`
//! (`crates/game/src/world.rs`), carried through save version 31
//! (`crates/game/src/world/save.rs`). This file checks both halves through
//! the real API: a second, unrelated faction must stay untouched, and the
//! witness state must round trip through an actual `save_state`/`load_state`
//! pair rather than being asserted only against the in-memory value.

use soe_esm::plugin::flags;
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::crime::CrimeType;
use soe_game::npc::{self, Config, Stats};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// `FACT.DATA`'s Track Crime bit (`uesp-modfileformat/FACT.wikitext`).
const TRACK_CRIME: u32 = 0x40;

/// A `FACT.CRVA` struct, 16 bytes: arrest through pickpocket, an unused
/// field, then the steal multiplier, the same layout
/// `prg_06_witnessed_crime_adversarial.rs` decodes.
fn crva16(murder: u16, assault: u16) -> Vec<u8> {
    let mut d = vec![0u8, 0u8];
    d.extend_from_slice(&murder.to_le_bytes());
    d.extend_from_slice(&assault.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes()); // trespass
    d.extend_from_slice(&0u16.to_le_bytes()); // pickpocket
    d.extend_from_slice(&0u16.to_le_bytes()); // unused
    d.extend_from_slice(&1.0f32.to_le_bytes()); // steal multiplier
    d
}

fn fact(form: u32, edid: &str, data_flags: u32, crva: Vec<u8>) -> Record {
    Record::new(T(b"FACT"), form)
        .zsub(T(b"EDID"), edid)
        .sub(T(b"DATA"), data_flags.to_le_bytes())
        .sub(T(b"CRVA"), crva)
}

fn npc_actor(form: u32, edid: &str, crime_faction: Option<u32>) -> Record {
    let rec = npc::build_record(
        form,
        edid,
        edid,
        Config {
            level: 1,
            ..Config::default()
        },
        Stats {
            health: 100,
            ..Stats::default()
        },
    );
    match crime_faction {
        Some(faction) => rec.sub(T(b"CRIF"), faction.to_le_bytes()),
        None => rec,
    }
}

fn achr(form: u32, edid: &str, base: u32) -> Record {
    Record::new(T(b"ACHR"), form)
        .zsub(T(b"EDID"), edid)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), vec![0u8; 24])
}

/// Wraps `cell` and `achrs` in the interior group nesting `World::actor`
/// needs to resolve a reference, the same shape
/// `prg_06_witnessed_crime_adversarial.rs` and `tests/npc.rs` build.
fn push_cell(p: &mut Plugin, cell: u32, achrs: Vec<Record>) {
    let cell_rec = p.push_record(
        Record::new(T(b"CELL"), cell)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(cell));
    for a in achrs {
        let idx = p.push_record(a);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(cell));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell_rec));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
}

/// A plugin with two tracked factions, `BanditFaction` (the perpetrator's
/// own crime faction, `CRVA` prices murder at 500) and `TownGuildFaction`
/// (unrelated to the perpetrator, `CRVA` prices murder at 900 so a mixed-up
/// write would be obvious), plus a witness and a perpetrator placed as
/// `ACHR` references.
fn two_faction_fixture() -> (Game, GlobalFormId, GlobalFormId, GlobalFormId, GlobalFormId) {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(fact(0x701, "BanditFaction", TRACK_CRIME, crva16(500, 0)));
    p.push_top(fact(0x710, "TownGuildFaction", TRACK_CRIME, crva16(900, 0)));
    p.push_top(npc_actor(0x800, "WitnessBase", None));
    p.push_top(npc_actor(0x703, "PerpBase", Some(0x701)));
    push_cell(
        &mut p,
        0x704,
        vec![
            achr(0x705, "PerpRef", 0x703),
            achr(0x706, "WitnessRef", 0x800),
        ],
    );
    let bytes = p.to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let game = Game::new(order);
    let witness = game.form("WitnessRef").unwrap();
    let perp = game.form("PerpRef").unwrap();
    let owning_faction = GlobalFormId {
        file: 0,
        object: 0x701,
    };
    let other_faction = GlobalFormId {
        file: 0,
        object: 0x710,
    };
    (game, witness, perp, owning_faction, other_faction)
}

/// "A witnessed crime changes only the owning faction bounty": a murder
/// priced against `BanditFaction` must leave the unrelated, equally
/// tracked `TownGuildFaction` at its untouched default. `TownGuildFaction`
/// prices murder higher (900 against `BanditFaction`'s 500), so this test
/// fails if the crime is priced or posted against the wrong faction, not
/// just if some faction changes.
#[test]
fn prg_06_witnessed_crime_changes_only_the_owning_faction_bounty() {
    let (game, witness, perp, owning_faction, other_faction) = two_faction_fixture();
    let mut w = game.world_mut();

    w.record_witnessed_crime(perp, witness, CrimeType::Murder)
        .expect("a valid witness and a perpetrator with a tracked crime faction must succeed");

    let owning = w.faction(owning_faction).unwrap();
    assert_eq!(
        owning.crime_gold_violent, 500,
        "the owning faction must be charged BanditFaction's own CRVA murder price"
    );
    assert_eq!(
        owning.crime_gold_nonviolent, 0,
        "a violent crime must not also post to the nonviolent ledger"
    );

    let other = w.faction(other_faction).unwrap();
    assert_eq!(
        other.crime_gold_violent, 0,
        "a faction the perpetrator does not report crime to must not be charged"
    );
    assert_eq!(
        other.crime_gold_nonviolent, 0,
        "a faction the perpetrator does not report crime to must not be charged"
    );
    assert_eq!(
        other.infamy_violent, 0,
        "an untouched faction's infamy must stay at its default too"
    );
}

/// "...and persists the known witness state": after `record_witnessed_crime`,
/// a real `save_state`/`load_state` round trip into a fresh `World` built
/// from the same load order must still know `witness` saw `perp` do it,
/// through the actual save format, not by asserting only the in-memory
/// value or by hand-editing serialized bytes.
#[test]
fn prg_06_witnessed_crime_persists_the_known_witness_state() {
    let (game, witness, perp, owning_faction, _other_faction) = two_faction_fixture();
    let bytes = {
        let mut w = game.world_mut();
        w.record_witnessed_crime(perp, witness, CrimeType::Murder)
            .unwrap();
        assert!(
            w.actor(witness).unwrap().witnessed_crimes.contains(&perp),
            "the witness must know about the perpetrator before the round trip even starts"
        );
        w.save_state()
    };

    // A second, independent World from the same load order, standing in
    // for the world after a real save and reload.
    let (game2, witness2, perp2, owning_faction2, _) = two_faction_fixture();
    assert_eq!(witness, witness2, "fixture must resolve identically");
    assert_eq!(perp, perp2, "fixture must resolve identically");
    assert_eq!(
        owning_faction, owning_faction2,
        "fixture must resolve identically"
    );
    let mut w2 = game2.world_mut();
    w2.load_state(&bytes)
        .expect("a save written by the current SAVE_VERSION must load back");

    assert!(
        w2.actor(witness2)
            .unwrap()
            .witnessed_crimes
            .contains(&perp2),
        "witnessed_crimes must survive an actual save_state/load_state round trip"
    );
    assert_eq!(
        w2.faction(owning_faction2).unwrap().crime_gold_violent,
        500,
        "the faction bounty the crime produced must round trip alongside the witness state"
    );
}
