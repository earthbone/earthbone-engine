// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, none, s};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 1));
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 2));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 10));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 11));
    plugin.push_top(Record::new(Tag::new(b"IMAD"), 20));
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 21));
    let mut order = LoadOrder::new();
    order.add("EffectShaders.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, shader: u32, name: &str, args: &[Value]) {
    let script_name = format!("Scr24Call{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("shader", "EffectShader");
    let mut operands = vec![s(name), ident("shader"), ident("unused")];
    for index in 0..args.len() {
        let arg = format!("arg{index}");
        function = function.param(&arg, "Var");
        operands.push(ident(&arg));
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(shader)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn active(game: &Game) -> Vec<(u32, u32, Option<f64>)> {
    game.world()
        .effect_shader_playbacks()
        .into_iter()
        .map(|playback| {
            (
                playback.shader.object,
                playback.target.object,
                playback.remaining,
            )
        })
        .collect()
}

#[test]
fn scr_24_play_defaults_to_infinite_and_stop_only_removes_its_pair() {
    let mut game = game();
    assert!(game.vm.natives.duplicates.iter().all(|(class, name, _)| {
        class != "EffectShader" || !matches!(name.as_str(), "Play" | "Stop")
    }));
    let target = Value::Object(game.object_of_form(id(10)));
    call(&mut game, 1, "Play", &[target]);
    assert_eq!(active(&game), vec![(1, 10, None)]);
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].shader, id(1));
    assert_eq!(requests[0].target, id(10));
    assert_eq!(requests[0].duration, -1.0);
    assert!(requests[0].playing);
}

#[test]
fn scr_24_play_duration_replacement_stop_and_expiry_are_observable() {
    let mut game = game();
    let target_a = Value::Object(game.object_of_form(id(10)));
    let target_b = Value::Object(game.object_of_form(id(11)));
    call(&mut game, 1, "Play", &[target_a.clone(), Value::Float(3.0)]);
    call(
        &mut game,
        1,
        "Play",
        &[target_b.clone(), Value::Float(-2.0)],
    );
    call(&mut game, 2, "Play", &[target_a.clone(), Value::Float(5.0)]);
    call(&mut game, 1, "Play", &[target_a.clone(), Value::Float(4.0)]);
    assert_eq!(
        active(&game),
        vec![(1, 10, Some(4.0)), (1, 11, None), (2, 10, Some(5.0))]
    );
    call(&mut game, 2, "Play", &[target_a.clone(), Value::Float(0.0)]);
    assert_eq!(active(&game), vec![(1, 10, Some(4.0)), (1, 11, None)]);
    call(&mut game, 1, "Stop", std::slice::from_ref(&target_a));
    assert_eq!(active(&game), vec![(1, 11, None)]);
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 6);
    assert!(!requests[4].playing);
    assert_eq!(requests[4].shader, id(2));
    assert_eq!(requests[4].target, id(10));
    assert!(!requests[5].playing);
    assert_eq!(requests[5].shader, id(1));
    assert_eq!(requests[5].target, id(10));
    game.tick(5.0);
    assert_eq!(active(&game), vec![(1, 11, None)]);
}

#[test]
fn scr_24_invalid_arguments_log_errors_without_changing_active_playbacks() {
    let mut game = game();
    let target = Value::Object(game.object_of_form(id(10)));
    call(&mut game, 1, "Play", &[target.clone(), Value::Float(4.0)]);
    let before = active(&game);
    game.world_mut().take_effect_shader_requests();
    let bad_target = Value::Object(game.object_of_form(id(20)));
    call(&mut game, 1, "Play", &[bad_target, Value::Float(1.0)]);
    call(
        &mut game,
        1,
        "Play",
        &[target.clone(), Value::Float(f32::NAN)],
    );
    call(
        &mut game,
        1,
        "Play",
        &[target.clone(), Value::string("forever")],
    );
    call(&mut game, 1, "Play", &[]);
    call(&mut game, 1, "Stop", &[]);
    assert_eq!(active(&game), before);
    for expected in [
        "target is not a reference",
        "non-finite effect shader duration",
        "duration must be numeric",
        "target must be a placed reference",
    ] {
        assert!(
            game.vm.log.contains(expected),
            "missing error {expected:?}: {:?}",
            game.vm.log.messages()
        );
    }
    assert!(game.world_mut().stop_effect_shader(id(20), id(10)).is_err());
    assert!(
        game.world_mut()
            .play_effect_shader(id(20), id(10), 1.0)
            .is_err()
    );
    assert!(
        game.world_mut()
            .play_effect_shader(id(1), id(10), f32::INFINITY)
            .is_err()
    );
    assert_eq!(active(&game), before);
    assert!(game.world_mut().take_effect_shader_requests().is_empty());
}

#[test]
fn scr_24_active_playback_round_trips_and_uses_saved_lifetime() {
    let mut original = game();
    original
        .world_mut()
        .play_effect_shader(id(1), id(10), 4.0)
        .unwrap();
    original
        .world_mut()
        .play_effect_shader(id(2), id(11), -1.0)
        .unwrap();
    original.tick(1.5);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert_eq!(active(&restored), active(&original));
    let requests = restored.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].shader, id(1));
    assert_eq!(requests[0].target, id(10));
    assert_eq!(requests[0].duration, 2.5);
    assert!(requests[0].playing);
    assert_eq!(requests[1].duration, -1.0);
    assert!(requests[1].playing);
    original.tick(2.5);
    restored.tick(2.5);
    assert_eq!(active(&restored), vec![(2, 11, None)]);
    assert_eq!(original.world().save_state(), restored.world().save_state());
}

#[test]
fn scr_24_unread_section_payload_preserves_live_state_and_renderer_requests() {
    let original = game();
    original
        .world_mut()
        .play_effect_shader(id(1), id(10), 4.0)
        .unwrap();
    let mut saved = original.world().save_state();
    let marker = b"\x06SCR-24";
    let size_offset = saved
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .unwrap()
        + marker.len();
    let payload_size = u32::from_le_bytes(saved[size_offset..size_offset + 4].try_into().unwrap());
    let payload_end = size_offset + 4 + payload_size as usize;
    saved.splice(payload_end..payload_end, [0]);
    saved[size_offset..size_offset + 4].copy_from_slice(&(payload_size + 1).to_le_bytes());

    let restored = game();
    restored
        .world_mut()
        .play_effect_shader(id(2), id(11), -1.0)
        .unwrap();
    let before = restored.world().save_state();
    let error = restored.world_mut().load_state(&saved).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("section SCR-24 at world left unread bytes")
    );
    assert_eq!(restored.world().save_state(), before);
    let requests = restored.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].shader, id(2));
    assert_eq!(requests[0].target, id(11));
    assert!(requests[0].playing);
}

#[test]
fn scr_24_dynamic_target_round_trips_into_a_fresh_world() {
    let original = game();
    let target = original.world_mut().place_at(id(21), id(10), false);
    original
        .world_mut()
        .play_effect_shader(id(1), target, 4.0)
        .unwrap();
    let saved = original.world().save_state();
    let restored = game();
    restored.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        restored.world().effect_shader_playbacks(),
        original.world().effect_shader_playbacks()
    );
    let requests = restored.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target, target);
    assert!(requests[0].playing);
}

#[test]
fn scr_24_absent_section_defaults_playback_and_stops_old_host_effects() {
    let mut saved = game().world().save_state();
    let mut without_sections = saved.clone();
    crate::common::legacy::strip_world_sections(&mut without_sections);
    let count_offset = without_sections.len();
    let count = u16::from_le_bytes(saved[count_offset..count_offset + 2].try_into().unwrap());
    let marker = b"\x06SCR-24";
    let section_offset = saved
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .unwrap();
    let size_offset = section_offset + marker.len();
    let payload_size = u32::from_le_bytes(saved[size_offset..size_offset + 4].try_into().unwrap());
    saved.drain(section_offset..size_offset + 4 + payload_size as usize);
    saved[count_offset..count_offset + 2].copy_from_slice(&(count - 1).to_le_bytes());

    let restored = game();
    restored
        .world_mut()
        .play_effect_shader(id(2), id(11), -1.0)
        .unwrap();
    restored.world_mut().load_state(&saved).unwrap();
    assert!(restored.world().effect_shader_playbacks().is_empty());
    let requests = restored.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].shader, id(2));
    assert_eq!(requests[0].target, id(11));
    assert!(!requests[0].playing);
}
