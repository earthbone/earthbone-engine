// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial Patrol fixtures follow xEdit PKC2 and the CK procedure inputs.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::ai::{AiState, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const A: u32 = 0x804;
const B: u32 = 0x805;
const C: u32 = 0x806;

#[derive(Clone, Copy)]
struct Settings {
    static_path: bool,
    omitted: u8,
    input_count: usize,
    static_is_float: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            static_path: false,
            omitted: 0,
            input_count: 6,
            static_is_float: false,
        }
    }
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn template(settings: Settings) -> Record {
    let rec = package::close_header(package::build_template(TEMPLATE, "WalkingPatrol"), None);
    let data = 0i32.to_le_bytes().to_vec();
    let data = [data, A.to_le_bytes().to_vec(), 0i32.to_le_bytes().to_vec()].concat();
    let rec = rec
        .zsub(T(b"ANAM"), "SingleRef")
        .sub(T(b"PTDA"), data)
        .sub(T(b"UNAM"), [7]);
    let rec = package::with_float_input(rec, 11, 0.0);
    let rec = package::with_bool_input(rec, 3, false);
    let rec = package::with_bool_input(rec, 9, false);
    let rec = package::with_bool_input(rec, 14, false);
    let rec = if settings.static_is_float {
        package::with_float_input(rec, 15, f32::from(settings.static_path))
    } else {
        package::with_bool_input(rec, 15, settings.static_path)
    };
    let rec = package::finish(rec);
    let mut inputs = [7, 11, 3, 9, 14, 15];
    for (position, input) in inputs.iter_mut().enumerate() {
        if settings.omitted & (1 << position) != 0 {
            *input = u8::MAX;
        }
    }
    package::with_procedure(rec, "Patrol", &inputs[..settings.input_count], false)
}

fn placed(form: u32, x: f32, next: Option<u32>) -> Record {
    let mut data = vec![0; 24];
    data[..4].copy_from_slice(&x.to_le_bytes());
    let mut rec = Record::new(T(b"REFR"), form)
        .sub(T(b"NAME"), 0u32.to_le_bytes())
        .sub(T(b"DATA"), data)
        .sub(T(b"XPRD"), 1.0f32.to_le_bytes())
        .sub(T(b"XPPA"), [])
        .sub(T(b"INAM"), 0u32.to_le_bytes());
    if let Some(next) = next {
        rec = rec.sub(T(b"XLKR"), next.to_le_bytes());
    }
    rec
}

fn game(settings: Settings) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(template(settings));
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
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24])
            .sub(T(b"XLKR"), A.to_le_bytes()),
    );
    plugin.push_top(placed(A, 110.0, Some(B)));
    plugin.push_top(placed(B, 220.0, Some(C)));
    plugin.push_top(placed(C, 330.0, None));
    let mut order = LoadOrder::new();
    order
        .add("Patrol.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

fn position(game: &mut Game) -> f32 {
    game.world_mut().reference(id(ACTOR)).unwrap().position[0]
}

fn state(game: &Game) -> AiState {
    game.world().actor_ref(id(ACTOR)).unwrap().ai.clone()
}

#[test]
fn false_static_pathing_completes_and_true_remains_unsupported() {
    let mut walking = game(Settings::default());
    for expected in [110.0, 110.0, 220.0, 220.0, 330.0, 330.0] {
        walking.tick(1.0);
        assert_eq!(position(&mut walking), expected);
    }
    assert!(state(&walking).completed.contains_key(&id(PACK)));

    let mut static_path = game(Settings {
        static_path: true,
        ..Settings::default()
    });
    static_path.tick(1.0);
    assert_eq!(position(&mut static_path), 0.0);
    assert!(matches!(
        state(&static_path).procedure,
        Procedure::Unsupported(_)
    ));
}

#[test]
fn every_missing_optional_suffix_matches_explicit_false_inputs() {
    let mut explicit = game(Settings::default());
    explicit.tick(2.0);
    for input_count in 2..=5 {
        let mut omitted = game(Settings {
            input_count,
            ..Settings::default()
        });
        omitted.tick(2.0);
        assert_eq!(position(&mut omitted), position(&mut explicit));
        assert_eq!(state(&omitted), state(&explicit));
    }
}

#[test]
fn missing_required_path_start_or_radius_stays_unsupported() {
    for required_index in [0, 1] {
        let mut game = game(Settings {
            omitted: 1 << required_index,
            ..Settings::default()
        });
        game.tick(1.0);
        assert_eq!(position(&mut game), 0.0);
        assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
        assert!(state(&game).completed.is_empty());
    }
}

#[test]
fn optional_input_with_wrong_authored_type_is_unsupported() {
    let mut game = game(Settings {
        static_is_float: true,
        ..Settings::default()
    });
    game.tick(1.0);
    assert_eq!(position(&mut game), 0.0);
    assert!(matches!(state(&game).procedure, Procedure::Unsupported(_)));
}

#[test]
fn omitted_optional_inputs_resume_after_engine_save_load() {
    let settings = Settings {
        input_count: 3,
        ..Settings::default()
    };
    let mut original = game(settings);
    original.tick(1.0);
    let saved = original.save_state().unwrap();
    let mut restored = game(settings);
    restored.load_state(&saved).unwrap();
    for expected in [110.0, 220.0, 220.0, 330.0] {
        original.tick(1.0);
        restored.tick(1.0);
        assert_eq!(position(&mut restored), expected);
        assert_eq!(state(&restored), state(&original));
    }
}

#[test]
fn installed_patrol_inputs_keep_authored_order_and_omit_only_optional_fields() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping AI-90 installed-data survey");
        return;
    };
    let bytes = std::fs::read(std::path::Path::new(&data).join("Skyrim.esm")).unwrap();
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut branches = 0;
    let mut optional_omissions = 0;
    for (id, _) in order.winners_of(T(b"PACK")) {
        let record = order.record(order.winner_ref(id).unwrap());
        let package = package::Package::from_record(record).unwrap();
        for branch in &package.tree {
            if branch.procedure.as_deref() != Some("Patrol") {
                continue;
            }
            assert!((2..=6).contains(&branch.inputs.len()), "PACK {id:?}");
            for position in 0..6 {
                let index = branch.inputs.get(position).copied().unwrap_or(u8::MAX);
                if index == u8::MAX {
                    assert!((2..=5).contains(&position), "PACK {id:?}");
                    optional_omissions += 1;
                    continue;
                }
                let value = package.input(index as i8).map(|input| input.value);
                let typed = match position {
                    0 => matches!(value, Some(package::DataValue::Target(_))),
                    1 => matches!(value, Some(package::DataValue::Float(_))),
                    _ => matches!(value, Some(package::DataValue::Bool(_))),
                };
                assert!(typed, "PACK {id:?} input {position}: {value:?}");
            }
            branches += 1;
        }
    }
    assert_eq!(branches, 45);
    assert!(optional_omissions > 0);
}
