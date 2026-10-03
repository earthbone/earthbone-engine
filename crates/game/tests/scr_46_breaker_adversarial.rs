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

fn data() -> Vec<u8> {
    let mut data = Vec::new();
    for value in [1.0_f32, 2.0, 3.0, 4.0, -5.0, 6.0, 7.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for value in [2_u32, 3, 0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data
}

fn full_data() -> Vec<u8> {
    let mut data = data();
    data.extend_from_slice(&4096_u32.to_le_bytes());
    data.extend_from_slice(&0.75_f32.to_le_bytes());
    data
}

fn game(data: Vec<u8>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SPGD"), 0x800)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"ICON"), b"effects/rain.dds\0".to_vec()),
    );
    let mut order = LoadOrder::new();
    order.add("Particle.esp", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, args: &[Value]) {
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "ShaderParticleGeometry");
    let mut operands = vec![s("Apply"), ident("target"), ident("unused")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Float");
        operands.push(ident(&name));
    }
    let script = ScriptBuilder::new("Scr46Layout", "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(0x800)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global("Scr46Layout", "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_46_texture_requires_the_texture_layout_extension() {
    let mut game = game(data());
    call(&mut game, &[Value::Float(0.0)]);
    assert!(game.world().shader_particle_layers().is_empty());
    assert!(game.vm.log.contains("invalid SPGD DATA size"));
}

#[test]
fn scr_46_apply_requires_its_authored_fade_argument() {
    let mut game = game(full_data());
    call(&mut game, &[]);
    assert!(game.world().shader_particle_layers().is_empty());
    assert!(game.vm.log.contains("Apply"));
}
