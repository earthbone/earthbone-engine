// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

fn fixture() -> Game {
    let tag = Tag::new;
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(tag(b"ACTI"), 0x800).zsub(tag(b"EDID"), "ShaderPhaseBase"));
    plugin.push_top(
        Record::new(tag(b"EFSH"), 0x801)
            .zsub(tag(b"EDID"), "ShaderPhase")
            .zsub(tag(b"ICON"), "textures\\effects\\start.dds")
            .zsub(tag(b"ICO2"), "textures\\effects\\loop.dds")
            .zsub(tag(b"NAM7"), "textures\\effects\\end.dds")
            .sub(tag(b"DATA"), vec![0; 0x134]),
    );
    plugin.push_top(
        Record::new(tag(b"REFR"), 0x802)
            .zsub(tag(b"EDID"), "ShaderPhaseTarget")
            .sub(tag(b"NAME"), 0x800_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add(
            "ShaderPhases.esm",
            Plugin::parse(&plugin.to_bytes()).unwrap(),
        )
        .unwrap();
    Game::new(order)
}

fn play(game: &mut Game, duration: f32) {
    let call = FunctionBuilder::new("Run", "Var")
        .global()
        .param("shader", "Form")
        .param("target", "Form")
        .param("duration", "Float")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![
                s("Play"),
                ident("shader"),
                ident("result"),
                ident("target"),
                ident("duration"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("ShaderPhasesProbe", "Form")
            .function(call)
            .bytes(),
    )
    .unwrap();
    let shader = game.object_of_form(game.form("ShaderPhase").unwrap());
    let target = game.object_of_form(game.form("ShaderPhaseTarget").unwrap());
    let thread = game
        .vm
        .call_global(
            "ShaderPhasesProbe",
            "Run",
            &[
                Value::Object(shader),
                Value::Object(target),
                Value::Float(duration),
            ],
        )
        .unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn effect_shader_play_preserves_start_loop_and_post_texture_groups() {
    let mut game = fixture();
    play(&mut game, 3.0);
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].appearance.fill_texture.as_deref(),
        Some("textures\\effects\\start.dds")
    );
    let appearance = format!("{:?}", requests[0].appearance);
    assert!(appearance.contains("loop.dds"));
    assert!(appearance.contains("end.dds"));
}

#[test]
fn repeated_effect_shader_play_keeps_each_lifetime_refresh_for_the_same_pair() {
    let mut game = fixture();
    play(&mut game, 2.0);
    play(&mut game, 8.0);
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.playing));
    assert!(
        requests
            .iter()
            .all(|request| request.shader.object == 0x801)
    );
    assert!(
        requests
            .iter()
            .all(|request| request.target.object == 0x802)
    );
    assert_eq!(requests[0].duration, 2.0);
    assert_eq!(requests[1].duration, 8.0);
}

#[test]
fn mag_14_efsh_fields_use_the_authored_membrane_offsets() {
    let game = fixture();
    let shader = soe_esm::GlobalFormId {
        file: 0,
        object: 0x801,
    };
    let plugin_record = game.world().effect_shader_appearance(shader);
    assert_eq!(
        plugin_record.fill_texture.as_deref(),
        Some("textures\\effects\\start.dds")
    );
    assert_eq!(plugin_record.color, [0; 4]);
    assert_eq!(plugin_record.blend, (0, 0));
    assert_eq!(plugin_record.fade, [0.0; 4]);
}
