// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cloned_ref_to_slice_refs
)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::{CallMethod, CallStatic, Return},
    Value,
};

const SPGD: GlobalFormId = GlobalFormId { file: 0, object: 1 };
const SOUND: GlobalFormId = GlobalFormId { file: 0, object: 2 };
const SOURCE: GlobalFormId = GlobalFormId { file: 0, object: 3 };

fn game(malformed_spgd: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut data = Vec::new();
    for value in [1.0_f32, 2.0, 3.0, 4.0, -5.0, 6.0, 7.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for value in [2_u32, 3, 0, 4096] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&0.75_f32.to_le_bytes());
    if malformed_spgd {
        data.truncate(8);
    }
    plugin.push_top(
        Record::new(Tag::new(b"SPGD"), SPGD.object)
            .sub(Tag::new(b"DATA"), data)
            .sub(Tag::new(b"ICON"), b"effects/rain.dds\0".to_vec()),
    );
    plugin.push_top(Record::new(Tag::new(b"SNDR"), SOUND.object));
    plugin.push_top(Record::new(Tag::new(b"REFR"), SOURCE.object));
    let mut order = LoadOrder::new();
    order.add("Scr70.esm", plugin).unwrap();
    Game::new(order)
}

fn call_method(
    game: &mut Game,
    class: &str,
    method: &str,
    target: GlobalFormId,
    args: &[Value],
) -> Value {
    let name = format!("Scr70{class}{method}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    let mut values = vec![Value::Object(game.object_of_form(target))];
    for (index, arg) in args.iter().enumerate() {
        let param = format!("arg{index}");
        let ty = match arg {
            Value::Float(_) => "Float",
            Value::Int(_) => "Int",
            Value::Bool(_) => "Bool",
            _ => "Var",
        };
        fun = fun.param(&param, ty);
        operands.push(ident(&param));
        values.push(arg.clone());
    }
    let script = ScriptBuilder::new(&name, "Form")
        .function(
            fun.ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game.vm.call_global(&name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn call_global(game: &mut Game, class: &str, method: &str, args: &[Value]) -> Value {
    let name = format!("Scr70{class}{method}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = vec![s(class), s(method), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let param = format!("arg{index}");
        let ty = match arg {
            Value::Float(_) => "Float",
            Value::Int(_) => "Int",
            _ => "Var",
        };
        fun = fun.param(&param, ty);
        operands.push(ident(&param));
    }
    let script = ScriptBuilder::new(&name, "Form")
        .function(
            fun.ins(CallStatic, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let thread = game.vm.call_global(&name, "Run", args).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_70_malformed_spgd_apply_does_not_create_a_layer() {
    let mut game = game(true);
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Apply",
        SPGD,
        &[Value::Float(0.0)],
    );
    assert!(game.world().shader_particle_layers().is_empty());
}

#[test]
fn scr_70_invalid_shader_fades_leave_existing_layer_unchanged() {
    let mut game = game(false);
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Apply",
        SPGD,
        &[Value::Float(0.0)],
    );
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Remove",
        SPGD,
        &[Value::Float(f32::NAN)],
    );
    assert_eq!(game.world().shader_particle_layers().len(), 1);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 1.0);
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Apply",
        SPGD,
        &[Value::Float(-1.0)],
    );
    assert_eq!(game.world().shader_particle_layers().len(), 1);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 1.0);
}

#[test]
fn scr_70_reapplying_shader_restarts_its_fade_in() {
    let mut game = game(false);
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Apply",
        SPGD,
        &[Value::Float(2.0)],
    );
    game.tick(1.0);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.5);
    call_method(
        &mut game,
        "ShaderParticleGeometry",
        "Apply",
        SPGD,
        &[Value::Float(4.0)],
    );
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.0);
    game.tick(2.0);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.5);
}

#[test]
fn scr_70_sound_play_rejects_a_missing_source_without_allocating_an_instance() {
    let mut game = game(false);
    let result = call_method(&mut game, "Sound", "Play", SOUND, &[Value::None]);
    assert_eq!(result, Value::Int(0));
    assert!(game.world().sound_instances.is_empty());
}

#[test]
fn scr_70_sound_instance_ids_remain_fresh_after_stop() {
    let mut game = game(false);
    let source = Value::Object(game.object_of_form(SOURCE));
    assert_eq!(
        call_method(&mut game, "Sound", "Play", SOUND, &[source.clone()]),
        Value::Int(1)
    );
    call_global(&mut game, "Sound", "StopInstance", &[Value::Int(1)]);
    assert_eq!(
        call_method(&mut game, "Sound", "Play", SOUND, &[source]),
        Value::Int(2)
    );
    assert_eq!(game.world().sound_instances.len(), 1);
    assert!(game.world().sound_instances.contains_key(&2));
}

#[test]
fn scr_70_sound_volume_stays_within_contract_for_nan() {
    let mut game = game(false);
    let source = Value::Object(game.object_of_form(SOURCE));
    call_method(&mut game, "Sound", "Play", SOUND, &[source]);
    call_global(
        &mut game,
        "Sound",
        "SetInstanceVolume",
        &[Value::Int(1), Value::Float(f32::NAN)],
    );
    let volume = game.world().sound_instances[&1].2;
    assert!(volume.is_finite() && (0.0..=1.0).contains(&volume));
}
