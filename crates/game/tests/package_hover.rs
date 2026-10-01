// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Hover uses the package's authored location and height, then holds until the
//! package no longer wins.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const DEST: u32 = 0x804;
const ENABLED: u32 = 0x805;
const TARGET: [f32; 3] = [50.0, 25.0, 10.0];
const HEIGHT: f32 = 40.0;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order(height: f32) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::finish(package::close_header(
        package::build_template(TEMPLATE, "Hover"),
        None,
    ));
    let template = package::with_procedure(template, "Hover", &[0, 1], false);
    let template = package::with_input_name(template, 0, "Target", true);
    let template = package::with_input_name(template, 1, "Height", true);
    plugin.push_top(package::with_actions(template));

    let enabled = Condition {
        comparison: condition::EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 74,
        param1: ENABLED,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let mut hover = package::with_condition(
        package::build_record(PACK, "HoverPackage", 0, Schedule::default()),
        &enabled,
    );
    hover = package::close_header(hover, Some(TEMPLATE));
    hover = package::with_location_input(hover, 0, Location::NearReference(DEST), 0);
    hover = package::with_float_input(hover, 1, height);
    plugin.push_top(package::with_actions(package::finish(hover)));

    plugin.push_top(
        npc::build_record(BASE, "Flyer", "Flyer", Config::default(), Stats::default())
            .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    for (tag, form, base, position) in [
        (*b"ACHR", ACTOR, BASE, [0.0; 3]),
        (*b"REFR", DEST, 0, TARGET),
    ] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            let start = axis * 4;
            data[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(&tag), form)
                .sub(T(b"NAME"), base.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    plugin.push_top(
        Record::new(T(b"GLOB"), ENABLED)
            .sub(T(b"FNAM"), b"f")
            .sub(T(b"FLTV"), 1.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("Hover.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game(height: f32) -> Game {
    let mut game = Game::new(order(height));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.tick(0.1);
    game
}

fn call_actor(game: &mut Game, name: &str) -> Value {
    let script = format!("Ai31Probe{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Actor")
        .local("result", "Var")
        .ins(CallMethod, vec![s(name), ident("actor"), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let actor = game.object_of_form(id(ACTOR));
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::Object(actor)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn ai_31_hover_tracks_authored_location_and_height_then_holds_in_flight() {
    let mut game = game(HEIGHT);
    {
        let world = game.world();
        let actor = world.actor_ref(id(ACTOR)).unwrap();
        assert_eq!(actor.ai.procedure, Procedure::Hover);
        assert_eq!(actor.ai.hover_height, HEIGHT);
        assert_eq!(
            actor.ai.destination,
            Some([TARGET[0], TARGET[1], TARGET[2] + HEIGHT])
        );
    }

    game.tick(2.0);
    assert!(game.world().actor_ref(id(ACTOR)).unwrap().ai.arrived);
    assert_eq!(call_actor(&mut game, "GetFlyingState"), Value::Int(3));
    assert_eq!(call_actor(&mut game, "IsFlying"), Value::Bool(true));
    assert_eq!(
        game.world_mut().reference(id(ACTOR)).unwrap().position[2],
        50.0
    );
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().ai.procedure,
        Procedure::Hover
    );
}

#[test]
fn ai_31_hover_cancellation_clears_flight_target_and_height() {
    let mut game = game(HEIGHT);
    game.world_mut().set_global(id(ENABLED), 0.0);
    game.world_mut().actor(id(ACTOR)).unwrap().ai.reevaluate = true;
    game.tick(0.0);

    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert_eq!(actor.ai.procedure, Procedure::None);
    assert_eq!(actor.ai.destination, None);
    assert_eq!(actor.ai.hover_height, 0.0);
    drop(world);
    assert_eq!(call_actor(&mut game, "GetFlyingState"), Value::Int(0));
    assert_eq!(call_actor(&mut game, "IsFlying"), Value::Bool(false));
}

#[test]
fn ai_31_hover_height_survives_world_save_and_future_versions_are_rejected() {
    let game = game(HEIGHT);
    let bytes = game.world().save_state();
    let restored = Game::new(order(HEIGHT));
    restored.world_mut().load_state(&bytes).unwrap();
    {
        let world = restored.world();
        let actor = world.actor_ref(id(ACTOR)).unwrap();
        assert_eq!(actor.ai.procedure, Procedure::Hover);
        assert_eq!(actor.ai.hover_height, HEIGHT);
        assert_eq!(
            actor.ai.destination,
            Some([TARGET[0], TARGET[1], TARGET[2] + HEIGHT])
        );
    }

    let mut legacy = bytes.clone();
    legacy[4..6].copy_from_slice(&33u16.to_le_bytes());
    assert!(restored.world_mut().load_state(&legacy).is_err());

    let mut future = bytes.clone();
    let future_version = soe_game::world::SAVE_VERSION + 1;
    future[4..6].copy_from_slice(&future_version.to_le_bytes());
    assert!(restored.world_mut().load_state(&future).is_err());
}

#[test]
fn ai_31_hover_rejects_nonfinite_authored_height() {
    let game = game(f32::NAN);
    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert!(matches!(actor.ai.procedure, Procedure::Unsupported(_)));
    assert_eq!(actor.ai.destination, None);
}

#[test]
fn ai_31_installed_hover_packages_resolve_their_authored_inputs() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let bytes = std::fs::read(format!("{data}/Skyrim.esm")).unwrap();
    let mut load_order = LoadOrder::new();
    load_order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let game = Game::new(load_order);
    let ids: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"PACK"))
        .map(|(id, _)| id)
        .collect();
    let world = game.world();
    let mut hover_count = 0;
    let mut invalid = Vec::new();
    for package in ids {
        match soe_game::ai::procedure_of(&world, package).0 {
            Procedure::Hover => hover_count += 1,
            Procedure::Unsupported(reason) if reason.contains("AI-31") => invalid.push(package),
            _ => {}
        }
    }
    assert!(hover_count > 0, "Skyrim.esm contains Hover package records");
    assert!(invalid.is_empty(), "Hover inputs unresolved: {invalid:?}");
}
