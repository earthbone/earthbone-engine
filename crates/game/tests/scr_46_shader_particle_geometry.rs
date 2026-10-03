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

fn spgd(object: u32, data: Vec<u8>) -> Record {
    Record::new(Tag::new(b"SPGD"), object)
        .sub(Tag::new(b"DATA"), data)
        .sub(Tag::new(b"ICON"), b"effects/rain.dds\0".to_vec())
}

fn data() -> Vec<u8> {
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
    plugin.push_top(spgd(1, data()));
    plugin.push_top(spgd(2, vec![0; 12]));
    let mut deleted = spgd(3, data());
    deleted.flags |= 0x20;
    plugin.push_top(deleted);
    plugin.push_top(spgd(4, data()));
    let mut order = LoadOrder::new();
    order.add("Particles.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, form: u32, method: &str, args: &[Value]) {
    let script_name = format!("Scr46Call{}", game.vm.time().to_bits());
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

fn layers(game: &Game) -> Vec<(u32, f32)> {
    game.world()
        .shader_particle_layers()
        .into_iter()
        .map(|layer| (layer.geometry.form.object, layer.strength))
        .collect()
}

#[test]
fn scr_46_apply_reads_spgd_arguments_and_fades_into_host_snapshot() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[Value::Float(2.0)]);
    let active = game.world().shader_particle_layers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].strength, 0.0);
    assert_eq!(active[0].geometry.particle_type, 0);
    assert_eq!(active[0].geometry.particle_density, 0.75);
    assert_eq!(
        active[0].geometry.texture.as_deref(),
        Some("effects/rain.dds")
    );
    game.tick(1.0);
    assert_eq!(layers(&game), vec![(1, 0.5)]);
    game.tick(1.0);
    assert_eq!(layers(&game), vec![(1, 1.0)]);
}

#[test]
fn scr_46_remove_fades_or_removes_the_matching_geometry() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[Value::Float(0.0)]);
    call(&mut game, 4, "Apply", &[Value::Float(0.0)]);
    assert_eq!(layers(&game), vec![(1, 1.0), (4, 1.0)]);
    call(&mut game, 1, "Remove", &[Value::Float(2.0)]);
    call(&mut game, 4, "Remove", &[Value::Float(0.0)]);
    assert_eq!(layers(&game), vec![(1, 1.0)]);
    game.tick(1.0);
    assert_eq!(layers(&game), vec![(1, 0.5)]);
    game.tick(1.0);
    assert!(game.world().shader_particle_layers().is_empty());
    call(&mut game, 1, "Remove", &[Value::Float(0.0)]);
    assert!(game.world().shader_particle_layers().is_empty());
}

#[test]
fn scr_46_invalid_data_and_fade_arguments_log_errors_without_partial_effects() {
    let mut game = game();
    call(&mut game, 1, "Apply", &[Value::Float(0.0)]);
    let before = layers(&game);
    assert_eq!(before, vec![(1, 1.0)]);
    call(&mut game, 2, "Apply", &[Value::Float(0.0)]);
    call(&mut game, 3, "Apply", &[Value::Float(0.0)]);
    call(&mut game, 1, "Apply", &[Value::Float(f32::NAN)]);
    call(&mut game, 1, "Apply", &[Value::Float(-0.25)]);
    call(&mut game, 1, "Remove", &[Value::Float(-1.0)]);
    call(&mut game, 1, "Remove", &[Value::Float(f32::INFINITY)]);
    assert_eq!(layers(&game), before);
    assert!(game.vm.log.contains("SPGD"));
    assert!(game.vm.log.contains("fade-in"));
    assert!(game.vm.log.contains("fade-out"));
}
