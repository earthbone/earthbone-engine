// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed MGEF inventory for MAG-68. xEdit leaves archetypes 13 and 14
//! unnamed, CommonLib calls them Darkness and NightEye, and Mutagen omits them.
//! Names do not establish execution semantics. See docs/game.md.

use crate::common::installed;

use std::collections::BTreeMap;

use soe_esm::Tag;
use soe_esm::records::gameplay::MagicEffect;

#[test]
fn mag_68_five_master_winning_effect_inventory_has_no_unnamed_archetypes() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-68 installed inventory");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let mut counts = BTreeMap::new();
    let mut unnamed = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect = MagicEffect::from_record(record, localized).unwrap();
        *counts.entry(effect.data.archetype).or_insert(0usize) += 1;
        if matches!(effect.data.archetype, 13 | 14) {
            unnamed.push((id, effect.data.archetype));
        }
    }
    let total: usize = counts.values().sum();
    eprintln!("MAG-68: {total} winning MGEFs, archetype counts {counts:?}");
    assert!(
        total > 1000,
        "inventory must cover the five-master effect corpus"
    );
    assert!(
        unnamed.is_empty(),
        "unnamed archetypes require investigation: {unnamed:?}"
    );
}
