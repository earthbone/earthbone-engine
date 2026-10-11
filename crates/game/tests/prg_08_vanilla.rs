// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Check that the shipped PERK records provide concrete priority and
//! cross-perk cases for the pending Skyrim runtime capture.

use crate::common::installed;
use soe_esm::{GlobalFormId, Tag};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
struct EntryPointSection {
    perk: GlobalFormId,
    entry_point: u8,
    priority: u8,
}

fn sections(record: &soe_esm::Record, perk: GlobalFormId) -> Vec<EntryPointSection> {
    let mut sections = Vec::new();
    let mut pending = None;
    for subrecord in &record.subrecords {
        if subrecord.tag == Tag(*b"PRKE") {
            pending = subrecord
                .data
                .get(..3)
                .filter(|data| data[0] == 2)
                .map(|data| (data[2], None));
        } else if subrecord.tag == Tag(*b"DATA") {
            if let Some((priority, entry_point)) = pending.as_mut() {
                if entry_point.is_none() {
                    *entry_point = subrecord.data.first().copied();
                    if let Some(entry_point) = *entry_point {
                        sections.push(EntryPointSection {
                            perk,
                            entry_point,
                            priority: *priority,
                        });
                    }
                }
            }
        } else if subrecord.tag == Tag(*b"PRKF") {
            pending = None;
        }
    }
    sections
}

#[test]
fn prg_08_vanilla_records_supply_priority_tie_and_cross_perk_capture_cases() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping PRG-08 vanilla PERK survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .expect("SKYRIM_DATA was checked");

    let mut observed = Vec::new();
    for (perk, record) in order
        .winners_of(Tag(*b"PERK"))
        .filter(|(_, record)| !record.is_deleted())
    {
        observed.extend(sections(record, perk));
    }
    let combat: Vec<_> = observed
        .into_iter()
        .filter(|section| matches!(section.entry_point, 35 | 36))
        .collect();

    let mut priorities_by_entry = BTreeMap::<u8, BTreeSet<u8>>::new();
    let mut perks_by_entry = BTreeMap::<u8, BTreeSet<GlobalFormId>>::new();
    let mut sections_by_perk_entry = BTreeMap::<(GlobalFormId, u8), Vec<u8>>::new();
    for section in &combat {
        priorities_by_entry
            .entry(section.entry_point)
            .or_default()
            .insert(section.priority);
        perks_by_entry
            .entry(section.entry_point)
            .or_default()
            .insert(section.perk);
        sections_by_perk_entry
            .entry((section.perk, section.entry_point))
            .or_default()
            .push(section.priority);
    }

    assert!(
        !combat.is_empty(),
        "expected vanilla ModAttackDamage or ModIncomingDamage entries"
    );
    assert!(
        priorities_by_entry
            .values()
            .any(|priorities| priorities.len() > 1),
        "expected a vanilla combat entry point with differing priorities"
    );
    assert!(
        sections_by_perk_entry.values().any(|priorities| {
            let mut seen = BTreeSet::new();
            priorities.iter().any(|priority| !seen.insert(priority))
        }),
        "expected same-priority sections within a vanilla PERK"
    );
    assert!(
        perks_by_entry.values().any(|perks| perks.len() > 1),
        "expected a combat entry point authored by multiple vanilla PERKs"
    );
}
