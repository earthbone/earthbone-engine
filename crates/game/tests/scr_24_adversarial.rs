// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 1));
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 4));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 2));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 3));
    let mut order = LoadOrder::new();
    order.add("Shaders.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, name: &str, args: &[Value]) {
    let script = format!("Scr24Adv{name}{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("shader", "EffectShader");
    let mut operands = vec![s(name), ident("shader"), ident("unused")];
    for (i, _) in args.iter().enumerate() {
        let param = format!("a{i}");
        function = function.param(&param, "Var");
        operands.push(ident(&param));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(1)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_24_zero_negative_and_repeated_stop_have_distinct_pair_semantics() {
    let mut game = fixture();
    let target = Value::Object(game.object_of_form(id(2)));
    call(&mut game, "Play", &[target.clone(), Value::Float(-0.25)]);
    assert_eq!(game.world().effect_shader_playbacks()[0].remaining, None);
    call(&mut game, "Play", &[target.clone(), Value::Float(0.0)]);
    assert!(game.world().effect_shader_playbacks().is_empty());
    call(&mut game, "Stop", std::slice::from_ref(&target));
    call(&mut game, "Stop", &[target]);
    assert!(game.world().effect_shader_playbacks().is_empty());
    let requests = game.world_mut().take_effect_shader_requests();
    assert_eq!(requests.len(), 4);
    assert!(requests[0].playing);
    assert!(!requests[1].playing);
    assert!(!requests[2].playing && !requests[3].playing);
}

#[test]
fn scr_24_expiry_removes_only_its_playback_and_save_retains_other_infinite_pair() {
    let mut original = fixture();
    let first = Value::Object(original.object_of_form(id(2)));
    original
        .world_mut()
        .play_effect_shader(id(1), id(2), 0.25)
        .unwrap();
    original
        .world_mut()
        .play_effect_shader(id(4), id(3), -1.0)
        .unwrap();
    call(&mut original, "Play", &[first, Value::Float(0.25)]);
    let saved = original.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    restored.tick(0.25);
    let active = restored.world().effect_shader_playbacks();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].shader, id(4));
    assert_eq!(active[0].target, id(3));
    assert_eq!(active[0].remaining, None);
}
