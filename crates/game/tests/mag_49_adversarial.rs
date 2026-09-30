// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{LoadOrder, Plugin, Tag};

#[test]
fn mag_49_installed_five_master_corpus_has_no_cure_addiction_contract() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, installed-data characterization skipped");
        return;
    };
    let mut order = LoadOrder::new();
    for name in ["Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm"] {
        order.add(name, Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap()).unwrap();
    }
    let mut effects = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"MGEF")) {
        if record.is_deleted() { continue; }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect = soe_esm::records::gameplay::MagicEffect::from_record(record, localized).unwrap();
        if effect.data.archetype == 28 { effects.push(id); }
    }
    assert!(order.winners_of(Tag::new(b"MGEF")).count() > 1000);
    assert!(effects.is_empty(), "Cure Addiction archetypes found: {effects:?}");
}
