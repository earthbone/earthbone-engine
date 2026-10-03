// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::{CallMethod, Return},
    Value,
};

fn shader_game() -> Game {
    let mut data = Vec::new();
    for value in [1.0_f32, 2.0, 3.0, 4.0, -5.0, 6.0, 7.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    for value in [2_u32, 3, 0, 4096] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&0.75_f32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag::new(b"SPGD"), 1)
            .sub(Tag::new(b"DATA"), data)
            .sub(Tag::new(b"ICON"), b"effects/rain.dds\0".to_vec()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr70.esm", plugin).unwrap();
    Game::new(order)
}

fn call_shader(game: &mut Game, method: &str, fade: Option<f32>) {
    let script_name = format!("Scr70{method}");
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "ShaderParticleGeometry")
        .local("result", "None");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    if fade.is_some() {
        function = function.param("fade", "Float");
        operands.push(ident("fade"));
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut args = vec![Value::Object(
        game.object_of_form(GlobalFormId { file: 0, object: 1 }),
    )];
    if let Some(fade) = fade {
        args.push(Value::Float(fade));
    }
    let thread = game.vm.call_global(&script_name, "Run", &args).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_70_sound_and_shader_bindings_keep_method_and_global_contracts() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();
    let methods: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| matches!(class.as_str(), "sound" | "shaderparticlegeometry"))
        .collect();
    assert_eq!(
        methods,
        vec![
            ("shaderparticlegeometry".to_string(), "apply".to_string()),
            ("shaderparticlegeometry".to_string(), "remove".to_string()),
            ("sound".to_string(), "play".to_string()),
        ]
    );

    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "sound")
        .collect();
    assert_eq!(
        globals,
        vec![
            ("sound".to_string(), "setinstancevolume".to_string()),
            ("sound".to_string(), "stopinstance".to_string()),
        ]
    );
    assert!(!game.vm.natives.has_method("Sound", "PlayAndWait"));
    assert!(!game.vm.natives.has_method("Sound", "GetDescriptor"));
}

#[test]
fn scr_70_shader_apply_uses_the_registered_instance_and_fade_argument() {
    let mut game = shader_game();
    call_shader(&mut game, "Apply", Some(2.0));
    let layers = game.world().shader_particle_layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].strength, 0.0);
    assert_eq!(
        layers[0].geometry.texture.as_deref(),
        Some("effects/rain.dds")
    );
    game.tick(1.0);
    assert_eq!(game.world().shader_particle_layers()[0].strength, 0.5);
}

#[test]
fn scr_70_shader_remove_targets_the_registered_instance() {
    let mut game = shader_game();
    call_shader(&mut game, "Apply", None);
    assert_eq!(game.world().shader_particle_layers().len(), 1);
    call_shader(&mut game, "Remove", None);
    assert!(game.world().shader_particle_layers().is_empty());
}
