// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn record(object: u32, data: Vec<u8>, texture: bool) -> Record {
    let mut record = Record::new(Tag::new(b"SPGD"), object).sub(Tag::new(b"DATA"), data);
    if texture {
        record = record.sub(Tag::new(b"ICON"), b"effects/rain.dds\0".to_vec());
    }
    record
}

fn full_data() -> Vec<u8> {
    let mut data = Vec::new();
    for value in [1.0_f32, 2.0, 3.0, 4.0, -5.0, 6.0, 7.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for value in [2_u32, 3, 0, 4096] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&0.75_f32.to_le_bytes());
    data
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record(1, full_data(), true));
    let mut short = full_data();
    short.truncate(40);
    plugin.push_top(record(2, short, false));
    let mut order = LoadOrder::new();
    order.add("Particles.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, form: u32, method: &str, args: &[Value]) {
    let script_name = format!("Scr46Breaker{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "ShaderParticleGeometry");
    let mut operands = vec![s(method), ident("target"), ident("unused")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Float");
        operands.push(ident(&name));
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(form)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_46_apply_requires_the_float_argument_listed_by_uesp() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[]);
    assert!(game.world().shader_particle_layers().is_empty());
    assert!(game.vm.log.contains("Apply"));
}

#[test]
fn scr_46_remove_requires_the_float_argument_listed_by_uesp() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[Value::Float(0.0)]);
    call(&mut game, 1, "Remove", &[]);
    assert_eq!(game.world().shader_particle_layers().len(), 1);
    assert!(game.vm.log.contains("Remove"));
}

#[test]
fn scr_46_optional_data_uses_the_format_defaults_without_texture() {
    let mut game = game();
    call(&mut game, 2, "Apply", &[Value::Float(0.0)]);
    let layers = game.world().shader_particle_layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].geometry.box_size, 4096);
    assert_eq!(layers[0].geometry.particle_density, 1.0);
    assert_eq!(layers[0].geometry.texture, None);
}

#[test]
fn scr_46_reapplying_during_fade_restarts_the_fade_from_zero() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[Value::Float(2.0)]);
    game.tick(1.0);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.5);
    call(&mut game, 1, "Apply", &[Value::Float(4.0)]);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.0);
    game.tick(1.0);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.25);
}
