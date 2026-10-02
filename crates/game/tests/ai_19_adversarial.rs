// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::AliasResolutionError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn alias_record(quest: u32, alias: u32, fill: &[(&[u8; 4], u32)]) -> Record {
    let mut record = Record::new(T(b"QUST"), quest)
        .sub(T(b"DNAM"), vec![0; 12])
        .sub(T(b"ALST"), alias.to_le_bytes())
        .zsub(T(b"ALID"), "Alias")
        .sub(T(b"FNAM"), 0u32.to_le_bytes());
    for (tag, value) in fill {
        record = record.sub(T(tag), value.to_le_bytes());
    }
    record.sub(T(b"ALED"), [])
}

fn game(records: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    plugin.push_top(Record::new(T(b"ACHR"), 0x900));
    let mut order = LoadOrder::new();
    order.add("AliasCases.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_19_missing_external_quest_or_alias_stays_unresolved_on_repeated_reads() {
    let quest = alias_record(0x800, 3, &[(b"ALEQ", 0x8FF), (b"ALEA", 9)]);
    let game = game(vec![quest]);
    for _ in 0..3 {
        assert_eq!(game.world_mut().resolve_alias_ref(id(0x800), 3), Ok(None));
    }
    assert_eq!(game.world_mut().alias_ref(id(0x800), 3), None);
}

#[test]
fn ai_19_external_dependency_cycle_reports_cycle_without_partial_alias_writes() {
    let first = alias_record(0x800, 0, &[(b"ALEQ", 0x801), (b"ALEA", 1)]);
    let second = alias_record(0x801, 1, &[(b"ALEQ", 0x800), (b"ALEA", 0)]);
    let game = game(vec![first, second]);
    assert_eq!(
        game.world_mut().resolve_alias_ref(id(0x800), 0),
        Err(AliasResolutionError::Cycle(vec![
            (id(0x800), 0),
            (id(0x801), 1),
            (id(0x800), 0),
        ]))
    );
    let mut world = game.world_mut();
    assert_eq!(
        world.quest_state(id(0x800)).unwrap().aliases.get(&0),
        Some(&None)
    );
    assert_eq!(
        world.quest_state(id(0x801)).unwrap().aliases.get(&1),
        Some(&None)
    );
}

#[test]
fn ai_19_external_quest_form_resolves_in_the_source_plugins_master_namespace() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(alias_record(0x801, 1, &[(b"ALFR", 0x900)]));
    base.push_top(Record::new(T(b"ACHR"), 0x900));
    let mut source = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..FileHeader::default()
    });
    source.push_top(alias_record(
        0x0100_0800,
        3,
        &[(b"ALEQ", 0x801), (b"ALEA", 1)],
    ));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Source.esp", source).unwrap();
    let game = Game::new(order);

    assert_eq!(
        game.world_mut().resolve_alias_ref(id(0x801), 1),
        Ok(Some(id(0x900)))
    );
    assert_eq!(
        game.world_mut().resolve_alias_ref(
            GlobalFormId {
                file: 1,
                object: 0x800
            },
            3,
        ),
        Ok(Some(id(0x900)))
    );
}
