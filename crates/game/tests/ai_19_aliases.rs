// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::AliasResolutionError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn alias_record(quest: u32, alias: u32, name: &str, fill: &[(&[u8; 4], u32)]) -> Record {
    let mut record = Record::new(T(b"QUST"), quest)
        .sub(T(b"DNAM"), vec![0; 12])
        .sub(T(b"ALST"), alias.to_le_bytes())
        .zsub(T(b"ALID"), name)
        .sub(T(b"FNAM"), 0u32.to_le_bytes());
    for (tag, value) in fill {
        record = record.sub(T(tag), value.to_le_bytes());
    }
    record.sub(T(b"ALED"), Vec::new())
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    plugin.push_top(Record::new(T(b"ACHR"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Ai19.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_19_alias_resolution_follows_local_external_dependency_order() {
    let source = alias_record(0x800, 0, "Local", &[(b"ALFA", 2)])
        .sub(T(b"ALST"), 2u32.to_le_bytes())
        .zsub(T(b"ALID"), "External")
        .sub(T(b"FNAM"), 0u32.to_le_bytes())
        .sub(T(b"ALEQ"), 0x801u32.to_le_bytes())
        .sub(T(b"ALEA"), 1u32.to_le_bytes())
        .sub(T(b"ALED"), Vec::new());
    let target = alias_record(0x801, 1, "Target", &[(b"ALFR", 0x900)]);
    let game = game(vec![source, target]);

    assert_eq!(
        game.world_mut().resolve_alias_ref(id(0x800), 0),
        Ok(Some(id(0x900)))
    );
    assert_eq!(
        game.world_mut().resolve_alias_ref(id(0x800), 2),
        Ok(Some(id(0x900)))
    );
}

#[test]
fn ai_19_alias_cycle_is_reported_without_partial_fills() {
    let first = alias_record(0x800, 0, "First", &[(b"ALFA", 1)])
        .sub(T(b"ALST"), 1u32.to_le_bytes())
        .zsub(T(b"ALID"), "Second")
        .sub(T(b"FNAM"), 0u32.to_le_bytes())
        .sub(T(b"ALFA"), 0u32.to_le_bytes())
        .sub(T(b"ALED"), Vec::new());
    let game = game(vec![first]);
    let original = game
        .world_mut()
        .quest_state(id(0x800))
        .unwrap()
        .aliases
        .clone();

    assert_eq!(
        game.world_mut().resolve_alias_ref(id(0x800), 0),
        Err(AliasResolutionError::Cycle(vec![
            (id(0x800), 0),
            (id(0x800), 1),
            (id(0x800), 0),
        ]))
    );
    let mut world = game.world_mut();
    let state = world.quest_state(id(0x800)).unwrap();
    assert_eq!(state.aliases, original);
}

#[test]
fn ai_19_cleared_dependency_stays_empty_after_save_load() {
    let records = vec![alias_record(0x800, 0, "Local", &[(b"ALFA", 1)])
        .sub(T(b"ALST"), 1u32.to_le_bytes())
        .zsub(T(b"ALID"), "Target")
        .sub(T(b"FNAM"), 0u32.to_le_bytes())
        .sub(T(b"ALFR"), 0x900u32.to_le_bytes())
        .sub(T(b"ALED"), [])];
    let original = game(records.clone());
    assert_eq!(
        original.world_mut().alias_ref(id(0x800), 0),
        Some(id(0x900))
    );
    assert!(original.world_mut().force_alias(id(0x800), 0, None));
    assert_eq!(
        original.world_mut().resolve_alias_ref(id(0x800), 0),
        Ok(None)
    );

    let save = original.save_state().unwrap();
    let mut restored = game(records);
    restored.load_state(&save).unwrap();
    assert_eq!(restored.world_mut().alias_ref(id(0x800), 0), None);
    assert_eq!(
        restored.world_mut().resolve_alias_ref(id(0x800), 0),
        Ok(None)
    );
    assert_eq!(
        restored.world_mut().alias_ref(id(0x800), 1),
        Some(id(0x900))
    );
}
