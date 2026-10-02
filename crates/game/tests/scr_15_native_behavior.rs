// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-15 native-level Actor behavior checks.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "ProbeBase",
        "ProbeBase",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(T(b"ACHR"), 0x900).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"IDLE"), 0x901).zsub(T(b"EDID"), "TestIdle"));
    let mut order = LoadOrder::new();
    order.add("SCR15.esm", plugin).unwrap();
    Game::new(order)
}

fn papyrus_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "Bool",
        Value::Float(_) => "Float",
        Value::Int(_) => "Int",
        Value::Object(_) | Value::None => "Form",
        _ => "Var",
    }
}

fn call(game: &mut Game, method: &str, args: &[Value], result_type: &str) -> Value {
    let probe = PROBES.fetch_add(1, Ordering::SeqCst);
    let class = format!("Scr15Probe{probe}");
    let mut function = FunctionBuilder::new("Run", result_type).param("target", "Form");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let name = format!("arg{index}");
        function = function.param(&name, papyrus_type(value));
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new(&class, "Form")
        .function(
            function
                .local("result", result_type)
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(id(0x900));
    game.vm.attach(object, &class).unwrap();
    let mut values = vec![Value::Object(object)];
    values.extend_from_slice(args);
    let thread = game.vm.call(object, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_15_get_voice_recovery_time_reads_the_actor_deadline() {
    let mut game = game();
    assert!(game.world_mut().set_voice_recovery_time(id(0x900), 7.5));
    assert_eq!(
        call(&mut game, "GetVoiceRecoveryTime", &[], "Float"),
        Value::Float(7.5)
    );
    assert!(game.world_mut().set_voice_recovery_time(id(0x900), 0.0));
    assert_eq!(
        call(&mut game, "GetVoiceRecoveryTime", &[], "Float"),
        Value::Float(0.0)
    );
}

#[test]
fn scr_15_play_idle_with_target_queues_actor_idle_and_rejects_bad_forms() {
    let mut game = game();
    let idle = Value::Object(game.object_of_form(id(0x901)));
    let target = Value::Object(game.object_of_form(id(0x900)));
    assert_eq!(
        call(
            &mut game,
            "PlayIdleWithTarget",
            &[idle.clone(), target],
            "Bool"
        ),
        Value::Bool(true)
    );
    assert_eq!(
        game.world().animation_requests,
        [soe_game::world::AnimationRequest {
            target: id(0x900),
            animation: "TestIdle".into(),
        }]
    );
    assert_eq!(
        call(
            &mut game,
            "PlayIdleWithTarget",
            &[Value::None, Value::None],
            "Bool"
        ),
        Value::Bool(false)
    );
    let wrong_idle_type = Value::Object(game.object_of_form(id(0x800)));
    assert_eq!(
        call(
            &mut game,
            "PlayIdleWithTarget",
            &[wrong_idle_type, Value::None],
            "Bool"
        ),
        Value::Bool(false)
    );
    let missing_target = Value::Object(game.object_of_form(id(0x999)));
    assert_eq!(
        call(
            &mut game,
            "PlayIdleWithTarget",
            &[idle, missing_target],
            "Bool"
        ),
        Value::Bool(false)
    );
    assert_eq!(game.world().animation_requests.len(), 1);
}

#[test]
fn scr_15_dismount_reports_and_clears_mount_state() {
    let mut game = game();
    assert_eq!(call(&mut game, "Dismount", &[], "Bool"), Value::Bool(false));
    game.world_mut().actor(id(0x900)).unwrap().riding_mount = true;
    assert_eq!(call(&mut game, "Dismount", &[], "Bool"), Value::Bool(true));
    assert!(!game.world_mut().actor(id(0x900)).unwrap().riding_mount);
    assert_eq!(call(&mut game, "Dismount", &[], "Bool"), Value::Bool(false));
}

#[test]
fn scr_15_trespass_sets_the_shared_actor_state() {
    let mut game = game();
    assert!(!game.world_mut().actor(id(0x900)).unwrap().trespassing);
    assert_eq!(call(&mut game, "Trespass", &[], "Var"), Value::None);
    assert!(game.world_mut().actor(id(0x900)).unwrap().trespassing);
}
