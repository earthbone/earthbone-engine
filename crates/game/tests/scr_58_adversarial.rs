// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

#[test]
fn faction_and_force_ref_if_empty_have_registry_derived_contract_comparisons() {
    let audit = include_str!("../../../docs/reference-audit.md");
    let faction_methods = [
        "SetAlly",
        "SetEnemy",
        "GetReaction",
        "SetReaction",
        "GetCrimeGold",
        "GetCrimeGoldViolent",
        "GetCrimeGoldNonViolent",
        "SetCrimeGold",
        "SetCrimeGoldViolent",
        "ModCrimeGold",
        "GetInfamy",
        "GetInfamyViolent",
        "GetInfamyNonViolent",
        "CanPayCrimeGold",
        "PlayerPayCrimeGold",
        "IsPlayerExpelled",
        "SetPlayerExpelled",
        "SetPlayerEnemy",
        "IsFactionInCrimeGroup",
        "SendAssaultAlarm",
        "SendPlayerToJail",
        "ModReaction",
    ];
    assert!(audit.contains("Faction and ForceRefIfEmpty native contracts, SCR-58"));
    for name in faction_methods {
        assert!(audit.contains(&format!("`Faction.{name}`")), "missing CK comparison for Faction.{name}");
    }
    assert!(audit.contains("`ReferenceAlias.ForceRefIfEmpty`"));
    assert!(audit.contains("SCR-59 owns reference-type validation"));
}

#[test]
fn crime_group_membership_requires_a_valid_linked_form_list() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"FACT"), 1).sub(Tag::new(b"CRGR"), 3_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag::new(b"FACT"), 2));
    plugin.push_top(Record::new(Tag::new(b"FLST"), 3).sub(Tag::new(b"LNAM"), 2_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag::new(b"FLST"), 4));
    let mut order = LoadOrder::new();
    order.add("scr58-breaker.esp", plugin).expect("plugin loads");
    let mut game = Game::new(order);
    assert!(game.world_mut().faction_in_crime_group(id(1), id(2)));
    assert!(!game.world_mut().faction_in_crime_group(id(1), id(4)));
    assert!(!game.world_mut().faction_in_crime_group(id(1), GlobalFormId { file: 0, object: 999 }));
}

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
