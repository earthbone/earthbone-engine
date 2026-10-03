// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::{ai, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const TOPIC: u32 = 0x806;
const OTHER_TOPIC: u32 = 0x807;
const FURNITURE: u32 = 0x808;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = x.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 20]);
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn ptdo(kind: u32, data: &[u8]) -> Vec<u8> {
    let mut result = kind.to_le_bytes().to_vec();
    result.extend_from_slice(data);
    result
}

fn order(topic_data: &[Vec<u8>], furniture: bool) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut template =
        package::close_header(package::build_template(TEMPLATE, "PatrolAdversarial"), None);
    let mut target = 0i32.to_le_bytes().to_vec();
    target.extend_from_slice(&A.to_le_bytes());
    target.extend_from_slice(&0i32.to_le_bytes());
    template = template
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), target)
        .sub(T(b"UNAM"), [7]);
    template = package::with_float_input(template, 11, 0.0);
    template = package::with_bool_input(template, 3, false);
    template = package::with_bool_input(template, 9, false);
    template = package::with_bool_input(template, 14, false);
    template = package::finish(package::with_bool_input(template, 15, false));
    template = package::with_procedure(template, "Patrol", &[7, 11, 3, 9, 14, 15], false);
    plugin.push_top(template);
    plugin.push_top(package::finish(package::close_header(
        package::build_record(PACK, "Patrol", 0, Schedule::default()),
        Some(TEMPLATE),
    )));
    plugin.push_top(
        npc::build_record(
            BASE,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"DIAL"), TOPIC).zsub(T(b"EDID"), "PatrolTopic"));
    plugin.push_top(Record::new(T(b"DIAL"), OTHER_TOPIC).zsub(T(b"EDID"), "OtherTopic"));
    if furniture {
        plugin.push_top(
            Record::new(T(b"FURN"), FURNITURE)
                .zsub(T(b"EDID"), "PatrolFurniture")
                .sub(T(b"MNAM"), 1u32.to_le_bytes())
                .sub(T(b"FNPR"), [1_u8, 0, 0, 0]),
        );
    }
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
    for (marker, next) in [(A, Some(B)), (B, None)] {
        let base = if furniture && marker == A {
            FURNITURE
        } else {
            0
        };
        let mut record = placed(
            *b"REFR",
            marker,
            base,
            if marker == A { 110.0 } else { 220.0 },
        )
        .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
        .sub(T(b"XPPA"), [])
        .sub(T(b"INAM"), 0u32.to_le_bytes());
        if marker == A {
            for data in topic_data {
                record = record.sub(T(b"PDTO"), data.clone());
            }
        }
        if let Some(next) = next {
            record = record.sub(T(b"XLKR"), next.to_le_bytes());
        }
        plugin.push_top(record);
    }
    let mut load_order = LoadOrder::new();
    load_order
        .add(
            "Ai88Adversarial.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    load_order
}

fn game(topic_data: &[Vec<u8>], furniture: bool) -> Game {
    let mut game = Game::new(order(topic_data, furniture));
    game.player();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn patrol_state(game: &mut Game) -> ai::AiState {
    game.world_mut().actor_ref(id(ACTOR)).unwrap().ai.clone()
}

#[test]
fn ai_88_uses_first_dial_and_ignores_subtype_entries() {
    let mut game = game(
        &[
            ptdo(1, b"SUBT"),
            ptdo(0, &TOPIC.to_le_bytes()),
            ptdo(0, &OTHER_TOPIC.to_le_bytes()),
        ],
        false,
    );
    game.tick(1.0);
    assert_eq!(
        game.world()
            .dialogue_session()
            .and_then(|session| session.topic),
        Some(id(TOPIC))
    );
    assert_eq!(patrol_state(&mut game).patrol.unwrap().current, id(A));
}

#[test]
fn ai_88_rejects_unresolved_or_non_dial_topic_references_before_route_start() {
    for topic in [0x900, BASE] {
        let mut game = game(&[ptdo(0, &topic.to_le_bytes())], false);
        game.tick(1.0);
        assert!(game.world().dialogue_session().is_none());
        assert!(matches!(
            patrol_state(&mut game).procedure,
            ai::Procedure::Unsupported(_)
        ));
    }
}

#[test]
fn ai_88_invalid_topic_kind_does_not_run_marker_actions() {
    let mut game = game(&[ptdo(2, &[0; 4])], true);
    game.tick(1.0);
    assert!(game.world().dialogue_session().is_none());
    assert_eq!(game.world().furniture_seat(id(ACTOR)), None);
    assert!(matches!(
        patrol_state(&mut game).procedure,
        ai::Procedure::Unsupported(_)
    ));
}

#[test]
fn ai_88_null_topic_reference_is_a_valid_no_op() {
    let mut game = game(&[ptdo(0, &0u32.to_le_bytes())], false);
    game.tick(1.0);
    assert!(game.world().dialogue_session().is_none());
    assert_eq!(patrol_state(&mut game).patrol.unwrap().current, id(A));
}

#[test]
fn ai_88_does_not_replace_an_open_dialogue_and_releases_furniture_before_advance() {
    let mut game = game(&[ptdo(0, &TOPIC.to_le_bytes())], true);
    let player = game.world_mut().player();
    game.begin_dialogue(player, id(ACTOR)).unwrap();
    game.tick(1.0);
    assert_eq!(game.world().dialogue_session().unwrap().speaker, player);
    assert_eq!(game.world().furniture_seat(id(ACTOR)), Some((id(A), 0)));
    game.close_dialogue();
    game.tick(1.0);
    assert_eq!(patrol_state(&mut game).patrol.unwrap().current, id(B));
    assert_eq!(game.world().furniture_seat(id(ACTOR)), None);
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn ai_88_saved_dwell_does_not_replay_a_closed_marker_topic() {
    let topic_data = vec![ptdo(0, &TOPIC.to_le_bytes())];
    let mut original = game(&topic_data, false);
    original.tick(1.0);
    assert_eq!(
        original
            .world()
            .dialogue_session()
            .and_then(|session| session.topic),
        Some(id(TOPIC))
    );
    original.close_dialogue();
    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order(&topic_data, false));
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world().save_state(), original.world().save_state());

    restored.tick(0.5);
    assert!(restored.world().dialogue_session().is_none());
    restored.tick(0.5);
    assert!(restored.world().dialogue_session().is_none());
    assert_eq!(patrol_state(&mut restored).patrol.unwrap().current, id(B));
}
