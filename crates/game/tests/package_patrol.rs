// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Walking Patrol fixtures follow xEdit PKC2, PTDA, XLKR and XPRD layouts.

use crate::common::legacy;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{AiState, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::Schedule;
use soe_game::Game;
use soe_game::{package, quest};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const C: u32 = 0x806;
const IDLE: u32 = 0x807;
const FURNITURE: u32 = 0x808;

#[derive(Clone, Copy)]
struct Settings {
    repeat: bool,
    nearest: bool,
    cyclic: bool,
    linked: bool,
    horse: bool,
    static_path: bool,
    omitted: bool,
    non_root_cycle: bool,
    action: bool,
    furniture: bool,
    null_topic_and_image_space: bool,
    radius: f32,
    dwell: f32,
    x: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            repeat: false,
            nearest: false,
            cyclic: false,
            linked: false,
            horse: false,
            static_path: false,
            omitted: false,
            non_root_cycle: false,
            action: false,
            furniture: false,
            null_topic_and_image_space: false,
            radius: 0.0,
            dwell: 1.0,
            x: 0.0,
        }
    }
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn template(settings: Settings, target: u32) -> Record {
    template_target(
        settings,
        if settings.linked { 3 } else { 0 },
        if settings.linked { 0 } else { target },
    )
}

fn template_target(settings: Settings, target_kind: i32, target: u32) -> Record {
    let rec = package::close_header(package::build_template(TEMPLATE, "WalkingPatrol"), None);
    let mut data = target_kind.to_le_bytes().to_vec();
    data.extend_from_slice(&if settings.linked { 0 } else { target }.to_le_bytes());
    data.extend_from_slice(&0i32.to_le_bytes());
    let rec = rec
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [7]);
    let rec = package::with_float_input(rec, 11, settings.radius);
    let rec = package::with_bool_input(rec, 3, settings.repeat);
    let rec = package::with_bool_input(rec, 9, settings.nearest);
    let rec = package::with_bool_input(rec, 14, settings.horse);
    let rec = package::finish(package::with_bool_input(rec, 15, settings.static_path));
    package::with_procedure(
        rec,
        "Patrol",
        &[7, 11, 3, 9, 14, if settings.omitted { 255 } else { 15 }],
        false,
    )
}

fn placed(tag: [u8; 4], form: u32, base: u32, x: f32) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    Record::new(T(&tag), form)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn markers(plugin: &mut Plugin, settings: Settings, namespace: u32, offset: f32) {
    for (object, x, next) in [
        (A, 110.0, Some(B)),
        (B, 220.0, Some(if settings.non_root_cycle { B } else { C })),
        (C, 330.0, settings.cyclic.then_some(A)),
    ] {
        let base = if settings.furniture && object == A {
            namespace | FURNITURE
        } else {
            0
        };
        let mut rec = placed(*b"REFR", namespace | object, base, x + offset);
        if settings.null_topic_and_image_space {
            rec = rec.sub(T(b"INAM"), 0x9876u32.to_le_bytes());
        }
        let mut rec = rec
            .sub(T(b"XPRD"), settings.dwell.to_le_bytes())
            .sub(T(b"XPPA"), [])
            .sub(
                T(b"INAM"),
                if settings.action { IDLE } else { 0 }.to_le_bytes(),
            );
        if settings.null_topic_and_image_space {
            rec = rec.sub(T(b"PDTO"), [0; 8]);
        }
        if let Some(next) = next {
            rec = rec.sub(T(b"XLKR"), (namespace | next).to_le_bytes());
        }
        plugin.push_top(rec);
    }
}

fn order(settings: Settings) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template(settings, A));
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
    if settings.action {
        plugin.push_top(Record::new(T(b"IDLE"), IDLE).zsub(T(b"EDID"), "PatrolIdle"));
    }
    if settings.furniture {
        plugin.push_top(
            Record::new(T(b"FURN"), FURNITURE)
                .zsub(T(b"EDID"), "PatrolFurniture")
                .sub(T(b"MNAM"), 1u32.to_le_bytes())
                .sub(T(b"FNPR"), 1u16.to_le_bytes()),
        );
    }
    plugin.push_top(placed(*b"ACHR", ACTOR, BASE, settings.x).sub(T(b"XLKR"), A.to_le_bytes()));
    markers(&mut plugin, settings, 0, 0.0);
    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(settings: Settings) -> Game {
    let game = Game::new(order(settings));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn state(game: &Game) -> AiState {
    game.world().actor_ref(id(ACTOR)).unwrap().ai.clone()
}
fn position(game: &Game) -> f32 {
    game.world_mut().reference(id(ACTOR)).unwrap().position[0]
}

#[test]
fn ai_03_visits_linked_markers_in_order_and_finishes_after_terminal_dwell() {
    for linked in [false, true] {
        let mut game = game(Settings {
            linked,
            ..Settings::default()
        });
        for (i, expected) in [110.0, 110.0, 220.0, 220.0, 330.0, 330.0]
            .into_iter()
            .enumerate()
        {
            game.tick(1.0);
            assert_eq!(position(&game), expected);
            assert_eq!(state(&game).tree.unwrap().finished, i == 5);
        }
        assert!(state(&game).completed.contains_key(&id(PACK)));
        game.tick(10.0);
        assert_eq!(position(&game), 330.0);
    }
}

#[test]
fn open_routes_reverse_but_closed_routes_wrap_even_without_repeat() {
    for cyclic in [false, true] {
        let mut game = game(Settings {
            cyclic,
            repeat: !cyclic,
            ..Settings::default()
        });
        for _ in 0..6 {
            game.tick(1.0);
        }
        assert_eq!(
            state(&game).patrol.as_ref().unwrap().current,
            id(if cyclic { A } else { B })
        );
        assert_eq!(state(&game).patrol.as_ref().unwrap().reverse, !cyclic);
        game.tick(2.0);
        assert_eq!(position(&game), if cyclic { 110.0 } else { 220.0 });
        assert!(state(&game).completed.is_empty());
    }
}

#[test]
fn nearest_start_uses_the_following_marker_and_terminal_entry_can_finish() {
    let mut game = game(Settings {
        nearest: true,
        x: 215.0,
        ..Settings::default()
    });
    game.tick(0.1);
    assert_eq!(state(&game).patrol.unwrap().current, id(C));
    assert_eq!(position(&game), 226.0);
    let mut terminal = self::game(Settings {
        nearest: true,
        x: 320.0,
        ..Settings::default()
    });
    terminal.tick(0.1);
    assert_eq!(position(&terminal), 320.0);
    assert!(state(&terminal).tree.unwrap().finished);
    let mut returning = self::game(Settings {
        nearest: true,
        repeat: true,
        x: 320.0,
        ..Settings::default()
    });
    returning.tick(0.1);
    assert_eq!(state(&returning).patrol.as_ref().unwrap().current, id(B));
    assert!(state(&returning).patrol.unwrap().reverse);
}

#[test]
fn radius_stops_walking_and_marker_dwell_uses_non_menu_real_seconds() {
    let mut game = game(Settings {
        radius: 10.0,
        dwell: 2.0,
        ..Settings::default()
    });
    game.world_mut().clock.timescale = 1000.0;
    game.tick(2.0);
    assert_eq!(position(&game), 100.0);
    assert_eq!(state(&game).patrol.as_ref().unwrap().dwell_until, Some(4.0));
    game.tick_menu_mode(100.0, true);
    game.tick(1.0);
    assert_eq!(state(&game).patrol.as_ref().unwrap().current, id(A));
    game.tick(1.0);
    assert_eq!(state(&game).patrol.as_ref().unwrap().current, id(B));
    assert_eq!(position(&game), 100.0);
    game.tick(1.0);
    assert_eq!(position(&game), 210.0);
}

#[test]
fn saved_patrol_resumes_movement_dwell_and_return_direction() {
    let settings = Settings {
        repeat: true,
        ..Settings::default()
    };
    for ticks in [0, 1, 7] {
        let mut original = game(settings);
        for _ in 0..ticks {
            original.tick(1.0);
        }
        original.tick(0.25);
        let saved = original.save_state().unwrap();
        let mut restored = Game::new(order(settings));
        restored.load_state(&saved).unwrap();
        for _ in 0..16 {
            original.tick(0.25);
            restored.tick(0.25);
            assert_eq!(state(&restored), state(&original));
            assert_eq!(restored.world().save_state(), original.world().save_state());
        }
    }
}

#[test]
fn unavailable_marker_stops_before_movement_and_does_not_complete() {
    let mut game = game(Settings::default());
    game.tick(0.25);
    let before = position(&game);
    game.world_mut().reference(id(A)).unwrap().deleted = true;
    game.tick(1.0);
    assert_eq!(position(&game), before);
    assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
    assert!(state(&game).completed.is_empty());
}

#[test]
fn moved_marker_replans_before_spending_the_movement_budget() {
    let mut game = game(Settings::default());
    game.tick(0.25);
    game.world_mut().reference(id(A)).unwrap().position[0] = -110.0;
    game.tick(0.25);
    assert_eq!(position(&game), 0.0);
    assert_eq!(state(&game).destination, Some([-110.0, 0.0, 0.0]));
}

#[test]
fn unsupported_actions_options_and_invalid_routes_hold_without_mutating_position() {
    for settings in [
        Settings {
            static_path: true,
            ..Settings::default()
        },
        Settings {
            non_root_cycle: true,
            ..Settings::default()
        },
        Settings {
            radius: -1.0,
            ..Settings::default()
        },
        Settings {
            radius: f32::NAN,
            ..Settings::default()
        },
        Settings {
            dwell: f32::INFINITY,
            ..Settings::default()
        },
    ] {
        let mut game = game(settings);
        game.tick(10.0);
        assert_eq!(position(&game), 0.0);
        assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
        assert!(state(&game).completed.is_empty());
    }
}

#[test]
fn winning_template_target_uses_its_defining_plugin_namespace() {
    let settings = Settings::default();
    let mut order = order(settings);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Patrol.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(template(settings, 0x01000804));
    markers(&mut patch, settings, 0x01000000, 1000.0);
    order
        .add("Route.esp", Plugin::parse(&patch.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(20.0);
    assert_eq!(position(&game), 1110.0);
    assert_eq!(
        state(&game).patrol.unwrap().current,
        GlobalFormId { file: 1, object: A }
    );
}

#[test]
fn patrol_hash_and_save_validation_retain_direction_and_deadline() {
    use soe_papyrus::vm::state::{Reader, Writer};
    let mut game = game(Settings::default());
    let original = game.world().save_state();
    let mut old = legacy::without_exclusive_branches(&mut game.world_mut(), &[id(ACTOR)]);
    legacy::truncate_to_version(&mut old, 22, |candidate| {
        self::game(Settings::default())
            .world_mut()
            .load_state(candidate)
            .is_ok()
    });
    game.world_mut().load_state(&old).unwrap();
    assert_eq!(game.world().save_state(), original);
    game.tick(1.0);
    let old_hash = game.world().state_hash();
    game.world_mut()
        .actor(id(ACTOR))
        .unwrap()
        .ai
        .patrol
        .as_mut()
        .unwrap()
        .reverse = true;
    assert_ne!(game.world().state_hash(), old_hash);
    let original = game.world().save_state();
    let mut old = legacy::without_exclusive_branches(&mut game.world_mut(), &[id(ACTOR)]);
    old[4..6].copy_from_slice(&22u16.to_le_bytes());
    assert!(game.world_mut().load_state(&old).is_err());
    assert_eq!(game.world().save_state(), original);
    let mut invalid = state(&game);
    invalid.patrol.as_mut().unwrap().dwell_until = Some(f64::NAN);
    let mut writer = Writer::new();
    soe_game::ai::write_state(&mut writer, &invalid);
    assert!(soe_game::ai::read_state(&mut Reader::new(&writer.out)).is_err());
}

#[test]
fn final_radius_approach_never_exceeds_a_small_tick_budget() {
    for radius in [0.0, 10.0] {
        let mut game = game(Settings {
            radius,
            x: 107.0 - radius,
            ..Settings::default()
        });
        let before = position(&game);
        game.tick(0.001);
        assert!((position(&game) - before - 0.11).abs() < 0.0001);
        assert!(!state(&game).arrived);
    }
}

#[test]
fn ai_91_alias_route_start_keeps_winning_namespace_and_saved_target() {
    const QUEST: u32 = 0x807;
    const ALIAS: u32 = 7;
    let settings = Settings::default();
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(template_target(settings, 4, ALIAS));
    base.push_top(
        package::finish(package::close_header(
            package::build_record(PACK, "Patrol", 0, Schedule::default()),
            Some(TEMPLATE),
        ))
        .sub(T(b"QNAM"), QUEST.to_le_bytes()),
    );
    base.push_top(
        npc::build_record(
            BASE,
            "Walker",
            "Walker",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    base.push_top(placed(*b"ACHR", ACTOR, BASE, 0.0));
    base.push_top(quest::build_record(QUEST, "RouteQuest", 0));
    markers(&mut base, settings, 0, 0.0);

    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&base.to_bytes()).unwrap())
        .unwrap();
    let mut route = Plugin::new(FileHeader {
        masters: vec!["Patrol.esm".into()],
        ..FileHeader::default()
    });
    route.push_top(template_target(settings, 4, ALIAS));
    route.push_top(quest::with_alias(
        quest::build_record(QUEST, "RouteQuest", 0),
        ALIAS,
        "RouteStart",
        Some(0x0100_0804),
    ));
    markers(&mut route, settings, 0x0100_0000, 1000.0);
    order
        .add("Route.esp", Plugin::parse(&route.to_bytes()).unwrap())
        .unwrap();

    let mut game = Game::new(order);
    assert_eq!(
        game.world_mut().alias_ref(id(QUEST), ALIAS),
        Some(GlobalFormId { file: 1, object: A })
    );
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(20.0);
    let selected = GlobalFormId { file: 1, object: A };
    assert!(
        state(&game).patrol.is_some(),
        "alias Patrol did not start: {:?}",
        state(&game).procedure
    );
    assert_eq!(state(&game).patrol.as_ref().unwrap().start, selected);
    assert_eq!(position(&game), 1110.0);

    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert_eq!(state(&game).patrol.unwrap().start, selected);
}

#[test]
fn off_graph_radius_arrival_retains_the_destination_cell_policy() {
    for x in [0.0, 105.0] {
        let mut game = game(Settings {
            radius: 10.0,
            x,
            ..Settings::default()
        });
        game.world_mut().reference(id(ACTOR)).unwrap().cell = Some(id(0x900));
        game.world_mut().reference(id(A)).unwrap().cell = Some(id(0x901));
        game.tick(2.0);
        assert!(state(&game).arrived);
        assert_eq!(
            game.world_mut().reference(id(ACTOR)).unwrap().cell,
            Some(id(0x901))
        );
    }
}

#[test]
fn marker_cell_change_invalidates_the_old_path_even_at_identical_coordinates() {
    let mut game = game(Settings::default());
    game.tick(0.25);
    game.world_mut().reference(id(A)).unwrap().cell = Some(id(0x901));
    game.tick(0.25);
    assert_eq!(state(&game).center_cell, Some(id(0x901)));
    assert_eq!(state(&game).path_cells, vec![Some(id(0x901))]);
}

#[test]
fn a_marker_removed_from_the_live_route_stops_before_another_movement_tick() {
    let mut game = game(Settings {
        nearest: true,
        ..Settings::default()
    });
    game.tick(0.25);
    assert_eq!(state(&game).patrol.unwrap().current, id(B));
    game.world_mut().reference(id(A)).unwrap().linked = vec![(None, id(C))];
    let before = position(&game);
    game.tick(1.0);
    assert_eq!(position(&game), before);
    assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
}

#[test]
fn null_topics_and_image_space_fields_do_not_hide_or_create_patrol_actions() {
    for action in [false, true] {
        let mut game = game(Settings {
            action,
            null_topic_and_image_space: true,
            ..Settings::default()
        });
        game.tick(1.0);
        assert!(!matches!(state(&game).procedure, Procedure::Unsupported(_)));
        assert_eq!(position(&game), 110.0);
    }
}

#[test]
fn ai_88_patrol_plays_idle_once_and_releases_furniture_before_advancing() {
    let mut idle = game(Settings {
        action: true,
        ..Settings::default()
    });
    idle.tick(1.0);
    assert_eq!(
        idle.world().animation_events,
        vec![(id(ACTOR), "PatrolIdle".to_string())]
    );
    idle.tick(0.5);
    assert_eq!(idle.world().animation_events.len(), 1);

    let mut furniture = game(Settings {
        furniture: true,
        dwell: 1.0,
        ..Settings::default()
    });
    furniture.tick(1.0);
    assert!(furniture.world().furniture_seat(id(ACTOR)).is_some());
    assert_eq!(state(&furniture).patrol.as_ref().unwrap().current, id(A));
    furniture.tick(1.0);
    assert_eq!(state(&furniture).patrol.as_ref().unwrap().current, id(B));
    assert_eq!(furniture.world().furniture_seat(id(ACTOR)), None);
}

#[test]
fn zero_dwell_yields_until_the_next_driver_tick() {
    let mut game = game(Settings {
        dwell: 0.0,
        ..Settings::default()
    });
    game.tick(1.0);
    assert_eq!(state(&game).patrol.as_ref().unwrap().current, id(A));
    assert_eq!(position(&game), 110.0);
    game.tick(0.0);
    assert_eq!(state(&game).patrol.as_ref().unwrap().current, id(B));
    assert_eq!(position(&game), 110.0);
}
