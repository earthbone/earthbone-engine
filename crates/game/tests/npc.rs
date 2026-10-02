// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Actor state from `NPC_` records: a synthetic base through the natives,
//! and the shipped Prisoner from Skyrim.esm when `SKYRIM_DATA` is set.

use crate::common::installed;

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::Game;
use soe_game::npc::{self, Config, Npc, Stats, flags as npc_flags};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(T(b"RACE"), 0x700).zsub(T(b"EDID"), "NordRace"));
    p.push_top(Record::new(T(b"FACT"), 0x701).zsub(T(b"EDID"), "GuardFaction"));
    p.push_top(Record::new(T(b"SPEL"), 0x702).zsub(T(b"EDID"), "Flames"));
    let config = Config {
        flags: npc_flags::PROTECTED,
        level: 7,
        health_offset: 10,
        template_flags: 0,
        ..Default::default()
    };
    let stats = Stats {
        health: 90,
        magicka: 60,
        stamina: 70,
        ..Default::default()
    };
    let rec = npc::build_record(0x703, "GuardBase", "Guard", config, stats)
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes());
    let rec = npc::with_faction(rec, 0x701, 2);
    let rec = npc::with_spell(rec, 0x702);
    p.push_top(rec);
    let cell = p.push_record(
        Record::new(T(b"CELL"), 0x704)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let achr = p.push_record(
        Record::new(T(b"ACHR"), 0x705)
            .zsub(T(b"EDID"), "GuardRef")
            .sub(T(b"NAME"), 0x703u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0u8; 24]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(0x704));
    temp.entries.push(Entry::Record(achr));
    let mut children = Group::new(GroupKind::CellChildren(0x704));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

#[test]
fn an_actor_takes_its_stats_from_the_base_record() {
    let bytes = plugin().to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Npc.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let game = Game::new(order);
    let guard = game.form("GuardRef").unwrap();
    let race = game.form("NordRace").unwrap();
    let faction = game.form("GuardFaction").unwrap();
    let spell = game.form("Flames").unwrap();
    let mut w = game.world_mut();
    assert_eq!(w.actor_value(guard, "Health"), 100.0);
    assert_eq!(w.base_actor_value(guard, "magicka"), 60.0);
    assert_eq!(w.actor_value(guard, "Stamina"), 70.0);
    let a = w.actor(guard).unwrap().clone();
    assert_eq!(a.level, 7);
    assert!(a.protected && !a.essential);
    assert_eq!(a.race, Some(race));
    assert_eq!(a.factions.get(&faction), Some(&2));
    assert!(a.spells.contains(&spell));
    let base = w.base_of(guard).unwrap();
    assert!(w.npc(base).unwrap().is_protected());
}

#[test]
fn the_prisoner_from_skyrim_esm() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = installed::load_order(&["Skyrim.esm"]).unwrap();
    let game = Game::from_shared_order(order);
    let w = game.world();
    for edid in ["Player", "Prisoner", "MQ101Alduin"] {
        let Some(id) = w.form(edid) else {
            eprintln!("{edid}: not in Skyrim.esm");
            continue;
        };
        let Some(npc) = w.npc(id) else {
            eprintln!(
                "{edid}: {} is not an NPC_ record",
                w.tag_of(id).map(|t| t.to_string()).unwrap_or_default()
            );
            continue;
        };
        let values = npc.base_values();
        eprintln!(
            "{edid} {}: level {} essential {} protected {} female {} race {:?} template {:?} flags {:#x}",
            npc.name
                .as_ref()
                .and_then(|t| t.plain().map(str::to_string))
                .unwrap_or_default(),
            npc.level_for(1),
            npc.is_essential(),
            npc.is_protected(),
            npc.is_female(),
            npc.race,
            npc.template,
            npc.config.template_flags
        );
        eprintln!(
            "  health {} magicka {} stamina {} onehanded {} sneak {} spells {} factions {} items {}",
            values[0].1,
            values[1].1,
            values[2].1,
            values[3].1,
            values[12].1,
            npc.spells.len(),
            npc.factions.len(),
            npc.items.len()
        );
        assert!(npc.race.is_some(), "{edid} has a race");
    }
    let player = w.form("Player").unwrap();
    let npc: Npc = w.npc(player).unwrap();
    assert_eq!(npc.editor_id.as_deref(), Some("Player"));
    assert!(npc.stats.is_some(), "the player base carries DNAM");
    assert!(npc.base_values()[0].1 > 0.0, "the player has base health");
}
