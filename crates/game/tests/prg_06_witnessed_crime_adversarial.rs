// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for PRG-06, `World::record_witnessed_crime`
//! (`crates/game/src/world/crime.rs`) and the `FACT.CRVA` table it prices
//! against (`crates/game/src/crime.rs`).
//!
//! Layout source: `TES5Edit/Core/wbDefinitionsTES5.pas` and
//! `uesp-modfileformat/FACT.wikitext` for `FACT.DATA` and `FACT.CRVA`,
//! including the shipped `MS08AlikrFaction` record, which sets the Track
//! Crime flag but carries no `CRVA` subrecord at all. Save format source:
//! `crates/game/src/world/save.rs`'s own version log, which documents
//! version 31 as the one that added `witnessed_crimes`.

use crate::common::legacy;

use soe_esm::plugin::flags;
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::crime::CrimeType;
use soe_game::npc::{self, Config, Stats};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// `FACT.DATA`'s Track Crime bit (`uesp-modfileformat/FACT.wikitext`).
const TRACK_CRIME: u32 = 0x40;

/// A `FACT.CRVA` struct, 16 bytes: arrest through pickpocket, an unused
/// field, then the steal multiplier. `CrimeValues::from_bytes` accepts 12,
/// 16 or 20 byte tables, fields kept in the same order as the struct grows.
fn crva16(
    arrest: u8,
    attack_on_sight: u8,
    murder: u16,
    assault: u16,
    trespass: u16,
    pickpocket: u16,
    steal_multiplier: f32,
) -> Vec<u8> {
    let mut d = vec![arrest, attack_on_sight];
    d.extend_from_slice(&murder.to_le_bytes());
    d.extend_from_slice(&assault.to_le_bytes());
    d.extend_from_slice(&trespass.to_le_bytes());
    d.extend_from_slice(&pickpocket.to_le_bytes());
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&steal_multiplier.to_le_bytes());
    d
}

fn fact(form: u32, edid: &str, data_flags: u32, crva: Option<Vec<u8>>) -> Record {
    let mut r = Record::new(T(b"FACT"), form)
        .zsub(T(b"EDID"), edid)
        .sub(T(b"DATA"), data_flags.to_le_bytes());
    if let Some(bytes) = crva {
        r = r.sub(T(b"CRVA"), bytes);
    }
    r
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
/// needs to resolve a reference, the same shape `tests/npc.rs` builds.
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

/// One plugin, one tracked faction with `crva`, a witness with no crime
/// faction of its own and a perpetrator reporting to that faction, both
/// placed as `ACHR` references so `World::actor` resolves them.
fn fixture(crva: Option<Vec<u8>>) -> (Game, GlobalFormId, GlobalFormId) {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(fact(0x701, "BanditFaction", TRACK_CRIME, crva));
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
    (game, witness, perp)
}

/// A second high-value theft must not crash the crime ledger.
///
/// `bounty()` (`crates/game/src/crime.rs`) already saturates its own
/// float-to-int cast instead of overflowing, and `record_witnessed_crime`
/// (`crates/game/src/world/crime.rs`) clamps the ledger at zero on the low
/// end with `.max(0)`, showing the intended contract is a bounded i32
/// total, not a crash. A legitimately priced theft of a high-value item, on
/// top of an existing nonzero bounty, drives the unchecked `+` in that same
/// line past `i32::MAX` and panics in this profile (`overflow-checks =
/// true`) instead of holding to that bound.
#[test]
fn a_second_high_value_theft_keeps_the_bounty_within_i32_instead_of_overflowing() {
    let (game, witness, perp) = fixture(Some(crva16(0, 0, 0, 0, 5, 0, 1.0)));
    let mut w = game.world_mut();
    w.record_witnessed_crime(perp, witness, CrimeType::Trespass)
        .unwrap();
    let total = w
        .record_witnessed_crime(
            perp,
            witness,
            CrimeType::Stealing {
                item_value: u32::MAX,
            },
        )
        .expect("a legitimately priced theft must not crash the crime ledger");
    assert!(
        (0..=i32::MAX).contains(&total),
        "bounty total {total} left its valid i32 range"
    );
}

/// A genuine version 29 save, from before this field existed, must still
/// load. `read_actor_state` gates the read behind `version >= 30`
/// (`crates/game/src/world/save.rs`), so an older save that never had
/// `witnessed_crimes` bytes at all should come back with an empty set, not
/// an error.
#[test]
fn a_genuine_pre_prg_06_save_loads_cleanly_with_no_witnessed_crimes() {
    let (game, witness, _perp) = fixture(Some(crva16(0, 0, 0, 0, 0, 0, 1.0)));
    let sentinel = GlobalFormId {
        file: 0x1234,
        object: 0xABCD_1234,
    };
    let mut w = game.world_mut();
    w.actor(witness).unwrap().witnessed_crimes.insert(sentinel);
    let mut bytes = legacy::without_recent_ai_fields(&mut w, &[witness]);

    // `witnessed_crimes` is a length-prefixed set, written last in
    // `write_actor_state`. With a single entry it serializes as a u32
    // count of 1 immediately followed by the 6-byte form id (u16 file,
    // u32 object, both little-endian), a unique marker for this fixture.
    let mut marker = 1u32.to_le_bytes().to_vec();
    marker.extend_from_slice(&sentinel.file.to_le_bytes());
    marker.extend_from_slice(&sentinel.object.to_le_bytes());
    let offsets: Vec<_> = bytes
        .windows(marker.len())
        .enumerate()
        .filter_map(|(offset, window)| (window == marker).then_some(offset))
        .collect();
    assert_eq!(
        offsets.len(),
        1,
        "witnessed_crimes sentinel marker is not unique in the buffer"
    );
    let end = offsets[0] + marker.len();
    // Version 32 writes the left and right hand spell presence bytes right
    // after witnessed_crimes, both unset for this witness.
    assert_eq!(
        bytes[end..end + 2],
        [0, 0],
        "an actor with no hand spells must write two unset presence bytes after its crime set"
    );
    // The right spell field is immediately followed by this actor's own
    // named save-section table, which a version 29 writer never emitted.
    let section_start = end + 2;
    let section_len = legacy::section_table_len(&bytes, section_start);
    bytes.drain(offsets[0]..section_start + section_len);
    // CONTEXT: Empty visit and repeat tables have identical encodings in this fixture.
    legacy::truncate_to_version(&mut bytes, 29, |candidate| {
        fixture(Some(crva16(0, 0, 0, 0, 0, 0, 1.0)))
            .0
            .world_mut()
            .load_state(candidate)
            .is_ok()
    });

    w.load_state(&bytes)
        .expect("a real version 29 save must still load");
    assert!(
        w.actor(witness).unwrap().witnessed_crimes.is_empty(),
        "a version 29 save cannot carry witnessed_crimes, load must not invent any"
    );
}

/// A DLC patch plugin overriding a base plugin's `FACT.CRVA` must price
/// crimes by the winning record, the load-order design rule
/// `skyrim-open-engine/CLAUDE.md` states for every system ("every system
/// reads forms keyed by resolved FormID from the load order"), applied to
/// `World::record`'s override resolution
/// (`crates/game/src/world/lookup.rs`). Dawnguard, HearthFires and
/// Dragonborn sit in the load order like any other plugin
/// (`skyrim-open-engine/CLAUDE.md`, "the DLC are in scope").
#[test]
fn a_dlc_patch_plugin_overrides_the_bounty_pricing_not_just_the_base_record() {
    let mut base = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    base.push_top(fact(
        0x701,
        "BanditFaction",
        TRACK_CRIME,
        Some(crva16(0, 0, 0, 0, 0, 400, 1.0)),
    ));
    base.push_top(npc_actor(0x800, "WitnessBase", None));
    base.push_top(npc_actor(0x703, "PerpBase", Some(0x701)));
    push_cell(
        &mut base,
        0x704,
        vec![
            achr(0x705, "PerpRef", 0x703),
            achr(0x706, "WitnessRef", 0x800),
        ],
    );

    let mut patch = Plugin::new(FileHeader {
        flags: flags::MASTER,
        masters: vec!["Base.esm".into()],
        ..Default::default()
    });
    patch.push_top(fact(
        0x701,
        "BanditFaction",
        TRACK_CRIME,
        Some(crva16(0, 0, 0, 0, 0, 999, 1.0)),
    ));

    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&base.to_bytes()).unwrap())
        .unwrap();
    order
        .add("Dawnguard.esm", Plugin::parse(&patch.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    let witness = game.form("WitnessRef").unwrap();
    let perp = game.form("PerpRef").unwrap();
    let mut w = game.world_mut();
    let total = w
        .record_witnessed_crime(perp, witness, CrimeType::Pickpocket)
        .unwrap();
    assert_eq!(
        total, 999,
        "Dawnguard's CRVA override must price the crime, not Base.esm's"
    );
}

/// `MS08AlikrFaction` in `Skyrim.esm` sets Track Crime with no `CRVA`
/// subrecord at all (`uesp-modfileformat/FACT.wikitext`). `World::crime_values`
/// (`crates/game/src/world/forms.rs`) already defaults to zero for a
/// missing table, so a real record shaped this way must still record the
/// crime, at zero bounty, not fail or panic.
#[test]
fn a_tracked_faction_with_no_crva_prices_the_crime_at_zero_not_an_error() {
    let (game, witness, perp) = fixture(None);
    let mut w = game.world_mut();
    let total = w
        .record_witnessed_crime(perp, witness, CrimeType::Trespass)
        .expect("Track Crime without CRVA is real vanilla shape (MS08AlikrFaction), not an error");
    assert_eq!(
        total, 0,
        "a missing CRVA table must price at zero, not a leftover default"
    );
}

/// Every check in `record_witnessed_crime` runs before anything is
/// mutated (`crates/game/src/world/crime.rs` doc comment: "an invalid
/// witness or perpetrator... all leave the world unchanged"). An invalid
/// witness must leave the perpetrator's own faction bounty untouched.
#[test]
fn an_invalid_witness_leaves_the_faction_bounty_untouched() {
    let (game, witness, perp) = fixture(Some(crva16(0, 0, 500, 0, 0, 0, 1.0)));
    let bogus_witness = GlobalFormId {
        file: 0,
        object: 0xDEAD,
    };
    let faction_id = GlobalFormId {
        file: 0,
        object: 0x701,
    };
    let mut w = game.world_mut();
    w.record_witnessed_crime(perp, witness, CrimeType::Murder)
        .expect("a valid witness must be able to seed a nonzero bounty first");
    let before = w.faction(faction_id).unwrap().crime_gold_violent;
    let err = w.record_witnessed_crime(perp, bogus_witness, CrimeType::Murder);
    assert!(err.is_err(), "a bogus witness reference must be rejected");
    let after = w.faction(faction_id).unwrap().crime_gold_violent;
    assert_eq!(
        after, before,
        "a rejected witness must leave the faction bounty unchanged"
    );
}
