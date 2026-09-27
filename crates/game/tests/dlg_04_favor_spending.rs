// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-04: a response that spends favor points (INFO response flag bit 14,
//! `ck-wiki` `CalculateFavorCost`, TES5Edit `wbDefinitionsTES5.pas` INFO DATA
//! flags) decrements the speaker's favor balance and the topic drops out of
//! `dialogue_choices` once that balance can no longer cover the response's
//! CNAM favor level.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

const BASE: u32 = 0x100;
const SPEAKER: u32 = 0x200;
const QUEST: u32 = 0x300;
const TOPIC: u32 = 0x400;
const INFO: u32 = 0x500;

/// INFO response DATA/ENAM flag bit 14, "Spends Favor Points".
const SPENDS_FAVOR: u16 = 1 << 14;
/// CNAM favor level: Small.
const FAVOR_SMALL: u8 = 1;
/// The actor value dlg_04 spends from (`docs/game.md`, `actor_values.rs`
/// index 89, vanilla "Favor Points Bonus").
const FAVOR_POINTS_AV: &str = "favorpointsbonus";
/// Engine-estimated fallback cost for a Small favor when the load order
/// carries no `FPCS` default-object override (`docs/game.md`).
const FAVOR_COST_SMALL_DEFAULT: f32 = 10.0;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn quest(form: u32) -> Record {
    Record::new(T(b"QUST"), form)
        .sub(T(b"DNAM"), vec![0u8; 12])
        .sub(T(b"INDX"), [0, 0, 0, 0])
}

fn topic(form: u32, quest: u32) -> Record {
    Record::new(T(b"DIAL"), form)
        .zsub(T(b"FULL"), "Will you do me a favor")
        .sub(T(b"QNAM"), quest.to_le_bytes())
        .sub(T(b"BNAM"), (form + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), [0, 0, 0, 0])
        .sub(T(b"SNAM"), *b"CUST")
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes())
}

fn add_branch(plugin: &mut Plugin, topic: u32, quest: Vec<u8>) {
    plugin.push_top(
        Record::new(T(b"DLBR"), topic + 0x1000)
            .sub(T(b"QNAM"), quest)
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 1u32.to_le_bytes())
            .sub(T(b"SNAM"), topic.to_le_bytes()),
    );
}

fn add_topic(plugin: &mut Plugin, topic: Record, infos: Vec<Record>) {
    let topic_id = topic.form_id;
    add_branch(plugin, topic_id, QUEST.to_le_bytes().to_vec());
    let mut group = Group::new(GroupKind::Top(T(b"DIAL")));
    group.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(topic_id));
    for info in infos {
        children
            .entries
            .push(Entry::Record(plugin.push_record(info)));
    }
    group.entries.push(Entry::Group(children));
    plugin.groups.push(group);
}

/// A single response that spends a Small favor. It carries no other gate,
/// so its availability tracks the favor balance alone.
fn favor_info(form: u32) -> Record {
    let mut enam = SPENDS_FAVOR.to_le_bytes().to_vec();
    enam.extend_from_slice(&0u16.to_le_bytes());
    let mut response = vec![0u8; 24];
    response[12] = 1;
    Record::new(T(b"INFO"), form)
        .sub(T(b"ENAM"), enam)
        .sub(T(b"CNAM"), [FAVOR_SMALL])
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), "Sure, I'll help.")
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [7, BASE] {
        plugin.push_top(
            Record::new(T(b"NPC_"), base)
                .zsub(T(b"EDID"), if base == 7 { "Player" } else { "Speaker" })
                .sub(T(b"ACBS"), vec![0u8; 24]),
        );
    }
    plugin.push_top(
        Record::new(T(b"ACHR"), SPEAKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0u8; 24]),
    );
    plugin.push_top(quest(QUEST));
    add_topic(&mut plugin, topic(TOPIC, QUEST), vec![favor_info(INFO)]);
    plugin
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order.add("Favor.esm", plugin()).unwrap();
    Game::new(order)
}

fn open(game: &mut Game) {
    let player = game.player();
    game.start_quest(id(QUEST));
    game.begin_dialogue(id(SPEAKER), player).unwrap();
}

/// A follower with no favor points cannot be asked: the topic is absent
/// from the start, before anything has been spent.
#[test]
fn dlg_04_favor_topic_unavailable_when_balance_empty() {
    let mut game = game();
    open(&mut game);

    let choices = game.dialogue_choices().unwrap();
    assert!(!choices.available.iter().any(|c| c.topic == id(TOPIC)));
}

/// Funding the speaker's favor balance makes the topic available, choosing
/// it decrements the balance by the response's Small favor cost, and once
/// the remaining balance can no longer cover another Small favor the topic
/// is unavailable again. This exercises every clause of dlg_04's done
/// check: the favor topic decrements the favor state, and it is
/// unavailable when exhausted (both before any funding and after it runs
/// out).
#[test]
fn dlg_04_favor_topic_decrements_and_exhausts() {
    let mut game = game();
    open(&mut game);

    game.world_mut()
        .actor(id(SPEAKER))
        .unwrap()
        .values
        .set_base(FAVOR_POINTS_AV, 1.5 * FAVOR_COST_SMALL_DEFAULT)
        .unwrap();

    let choices = game.dialogue_choices().unwrap();
    assert!(
        choices.available.iter().any(|c| c.topic == id(TOPIC)),
        "a funded speaker should offer the favor topic"
    );

    game.choose_dialogue(id(TOPIC)).unwrap();
    assert!(
        game.advance_dialogue().unwrap().is_none(),
        "the single TRDT response should finish the line"
    );
    let remaining = game.world_mut().actor_value(id(SPEAKER), FAVOR_POINTS_AV);
    assert_eq!(
        remaining,
        0.5 * FAVOR_COST_SMALL_DEFAULT,
        "choosing the response should decrement the favor balance by its cost"
    );

    let choices = game.dialogue_choices().unwrap();
    assert!(
        !choices.available.iter().any(|c| c.topic == id(TOPIC)),
        "a balance below the response's cost should drop the topic"
    );
}
