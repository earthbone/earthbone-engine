// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Runtime movement checks for SCR-14 Actor offset and landing-marker state.

use soe_esm::{
    plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::ai::Procedure;
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn actor(form: u32, name: &str) -> Record {
    npc::build_record(
        form,
        name,
        name,
        npc::Config::default(),
        npc::Stats::default(),
    )
}

fn reference(form: u32, name: &str, base: u32) -> Record {
    Record::new(T(b"ACHR"), form)
        .zsub(T(b"EDID"), name)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), vec![0; 24])
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(actor(0x702, "GuardBase"));
    plugin.push_top(actor(0x703, "CriminalBase"));
    let cell = plugin.push_record(
        Record::new(T(b"CELL"), 0x704)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(0x704));
    for record in [
        reference(0x705, "GuardRef", 0x702),
        reference(0x706, "CriminalRef", 0x703),
    ] {
        let index = plugin.push_record(record);
        temporary.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(0x704));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add(
            "SCR14Breaker.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    Game::new(order)
}

fn call(game: &mut Game, target: Value, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr14BreakerProbe{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(
            &parameter,
            match value {
                Value::Bool(_) => "Bool",
                Value::Int(_) => "Int",
                Value::Object(_) | Value::None => "Form",
                _ => "Var",
            },
        );
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut arguments = vec![target];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &arguments).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn object(game: &mut Game, id: GlobalFormId) -> Value {
    Value::Object(game.object_of_form(id))
}

fn materialize(game: &mut soe_game::Game, id: GlobalFormId) {
    let _ = game.world_mut().reference(id);
    assert!(game.world_mut().drive_actor(id));
}

#[test]
fn breaker3_adversarial_case_keep_offset_drives_rotated_world_destination_and_angles() {
    let mut game = game();
    let actor = game.form("GuardRef").unwrap();
    let target = game.form("CriminalRef").unwrap();
    materialize(&mut game, actor);
    materialize(&mut game, target);
    {
        let mut world = game.world_mut();
        let target_ref = world.reference(target).unwrap();
        target_ref.position = [30.0, 40.0, 5.0];
        target_ref.rotation = [0.0, 0.0, std::f32::consts::FRAC_PI_2];
        world.reference(actor).unwrap().position = [30.0, 40.0, 5.0];
        world.actor(actor).unwrap().ai.procedure = Procedure::Sandbox;
        world.actor(actor).unwrap().ai.arrived = true;
    }
    let actor_object = object(&mut game, actor);
    let target_object = object(&mut game, target);
    call(
        &mut game,
        actor_object,
        "KeepOffsetFromActor",
        &[
            target_object,
            Value::Float(10.0),
            Value::Float(0.0),
            Value::Float(2.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(90.0),
            Value::Float(20.0),
            Value::Float(1.0),
        ],
    );

    for _ in 0..80 {
        game.tick(0.1);
    }

    let mut world = game.world_mut();
    let actor_ref = world.reference(actor).unwrap();
    assert!(
        (actor_ref.position[0] - 30.0).abs() < 1.0,
        "rotated body offset should place the actor east/west at x=30, got {:?}",
        actor_ref.position
    );
    assert!(
        actor_ref.position[1] > 48.0,
        "positive local X follows target yaw toward positive Y, got {:?}",
        actor_ref.position
    );
    assert!(
        (actor_ref.rotation[2] - std::f32::consts::PI).abs() < 0.05,
        "relative yaw should be applied in radians, got {:?}",
        actor_ref.rotation
    );
}

#[test]
fn breaker3_adversarial_case_arrived_flyer_still_routes_to_forced_landing_marker() {
    let mut game = game();
    let actor = game.form("GuardRef").unwrap();
    let marker = game.form("CriminalRef").unwrap();
    materialize(&mut game, actor);
    materialize(&mut game, marker);
    {
        let mut world = game.world_mut();
        world.reference(actor).unwrap().position = [0.0; 3];
        world.reference(marker).unwrap().position = [100.0, 0.0, 0.0];
        world.actor(actor).unwrap().ai.procedure = Procedure::Sandbox;
        world.actor(actor).unwrap().ai.destination = None;
        world.actor(actor).unwrap().ai.arrived = true;
        world.actor(actor).unwrap().flying = true;
    }
    let actor_object = object(&mut game, actor);
    let marker_object = object(&mut game, marker);
    call(
        &mut game,
        actor_object,
        "SetForcedLandingMarker",
        &[marker_object],
    );

    for _ in 0..80 {
        game.tick(0.1);
    }

    let mut world = game.world_mut();
    let actor_state = world.actor(actor).unwrap();
    assert_eq!(
        actor_state.forced_landing_marker,
        Some(marker),
        "landing marker remains the actor's selected marker"
    );
    assert!(
        world.reference(actor).unwrap().position[0] > 90.0,
        "an arrived flyer should land at the selected marker, got {:?}",
        world.reference(actor).unwrap().position
    );
}
