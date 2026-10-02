// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-52 tests for VisualEffect.Play and Stop argument handling, errors,
//! stacked instances, duration expiry and selective stop behavior.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::VisualEffectError;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EFFECT_A: u32 = 0x100;
const EFFECT_B: u32 = 0x101;
const BASE: u32 = 0x200;
const TARGET_A: u32 = 0x300;
const TARGET_B: u32 = 0x301;
const FACING: u32 = 0x302;

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn form_arg(game: &mut Game, edid: &str) -> Value {
    let id = game.form(edid).unwrap();
    Value::Object(game.object_of_form(id))
}

fn test_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        author: "soe".into(),
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"RFCT"), EFFECT_A).zsub(T(b"EDID"), "EffectA"));
    plugin.push_top(Record::new(T(b"RFCT"), EFFECT_B).zsub(T(b"EDID"), "EffectB"));
    plugin.push_top(Record::new(T(b"MISC"), BASE).zsub(T(b"EDID"), "Base"));
    for (reference, edid) in [
        (TARGET_A, "TargetA"),
        (TARGET_B, "TargetB"),
        (FACING, "Facing"),
    ] {
        plugin.push_top(
            Record::new(T(b"REFR"), reference)
                .zsub(T(b"EDID"), edid)
                .sub(T(b"NAME"), BASE.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("SCR52.esp", plugin).unwrap();
    Game::new(order)
}

fn value_type(value: &Value) -> &'static str {
    match value {
        Value::Float(_) => "Float",
        Value::None | Value::Object(_) => "Form",
        other => panic!("unexpected probe argument {other:?}"),
    }
}

fn call(game: &mut Game, effect: Value, name: &str, args: &[Value]) {
    let n = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr52Probe{n}");
    let mut fun = FunctionBuilder::new("Run", "None")
        .global()
        .param("effect", "Form");
    let mut operands = vec![s(name), ident("effect"), ident("result")];
    fun = fun.local("result", "Var");
    for (index, arg) in args.iter().enumerate() {
        let parameter = format!("a{index}");
        fun = fun.param(&parameter, value_type(arg));
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(fun.ins(CallMethod, operands).ins(Return, vec![]).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut inputs = vec![effect];
    inputs.extend_from_slice(args);
    game.vm.call_global(&script, "Run", &inputs).unwrap();
    game.run_until_idle(64);
}

#[test]
fn scr_52_play_defaults_stacks_instances_and_expires_duration() {
    let mut game = test_game();
    let effect = form_arg(&mut game, "EffectA");
    let target = form_arg(&mut game, "TargetA");
    let facing = form_arg(&mut game, "Facing");

    call(
        &mut game,
        effect.clone(),
        "Play",
        std::slice::from_ref(&target),
    );
    call(
        &mut game,
        effect,
        "Play",
        &[target.clone(), Value::Float(2.0), facing.clone()],
    );
    let instances: Vec<_> = game.world().visual_effect_instances().cloned().collect();
    assert_eq!(instances.len(), 2);
    assert_eq!(instances[0].effect, id(EFFECT_A));
    assert_eq!(instances[0].target, id(TARGET_A));
    assert_eq!(instances[0].remaining, None);
    assert_eq!(instances[0].facing, None);
    assert_eq!(instances[1].remaining, Some(2.0));
    assert_eq!(instances[1].facing, Some(id(FACING)));
    assert_ne!(instances[0].id, instances[1].id);

    game.tick(2.0);
    let instances: Vec<_> = game.world().visual_effect_instances().cloned().collect();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].remaining, None);
}

#[test]
fn scr_52_play_rejects_missing_and_invalid_arguments_without_mutation() {
    let mut game = test_game();
    let effect = form_arg(&mut game, "EffectA");
    let target = form_arg(&mut game, "TargetA");
    let bad_facing = Value::Float(4.0);

    call(&mut game, effect.clone(), "Play", &[]);
    call(
        &mut game,
        effect,
        "Play",
        &[target, Value::Float(1.0), bad_facing],
    );
    assert!(game.world().visual_effect_instances().next().is_none());
    let diagnostics: Vec<_> = game
        .traces()
        .into_iter()
        .filter(|trace| trace.contains("VisualEffect.Play"))
        .collect();
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");

    let mut world = game.world_mut();
    assert_eq!(
        world.play_visual_effect(id(EFFECT_A), id(TARGET_A), f64::NAN, None),
        Err(VisualEffectError::Duration)
    );
    assert_eq!(
        world.play_visual_effect(id(BASE), id(TARGET_A), -1.0, None),
        Err(VisualEffectError::Effect(id(BASE)))
    );
    assert_eq!(
        world.play_visual_effect(id(EFFECT_A), id(BASE), -1.0, None),
        Err(VisualEffectError::Target(id(BASE)))
    );
    assert!(world.visual_effect_instances().next().is_none());
}

#[test]
fn scr_52_stop_removes_only_matching_effect_target_instances() {
    let mut game = test_game();
    let effect_a = form_arg(&mut game, "EffectA");
    let effect_b = form_arg(&mut game, "EffectB");
    let target_a = form_arg(&mut game, "TargetA");
    let target_b = form_arg(&mut game, "TargetB");
    for (effect, target) in [
        (effect_a.clone(), target_a.clone()),
        (effect_a.clone(), target_a.clone()),
        (effect_a.clone(), target_b),
        (effect_b, target_a.clone()),
    ] {
        call(&mut game, effect, "Play", &[target]);
    }

    call(&mut game, effect_a, "Stop", &[target_a]);
    let instances: Vec<_> = game.world().visual_effect_instances().cloned().collect();
    assert_eq!(instances.len(), 2);
    assert!(instances
        .iter()
        .any(|v| v.effect == id(EFFECT_A) && v.target == id(TARGET_B)));
    assert!(instances
        .iter()
        .any(|v| v.effect == id(EFFECT_B) && v.target == id(TARGET_A)));
}

#[test]
fn scr_52_stop_reports_invalid_target_and_api_errors() {
    let mut game = test_game();
    let effect = form_arg(&mut game, "EffectA");
    call(&mut game, effect, "Stop", &[]);
    let effect = form_arg(&mut game, "EffectA");
    let base = form_arg(&mut game, "Base");
    call(&mut game, effect, "Stop", &[base]);
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("VisualEffect.Stop: missing target")));
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("VisualEffect.Stop: visual effect target")));
    assert_eq!(
        game.world_mut().stop_visual_effect(id(EFFECT_A), id(BASE)),
        Err(VisualEffectError::Target(id(BASE)))
    );
    assert_eq!(
        game.world_mut().stop_visual_effect(id(BASE), id(TARGET_A)),
        Err(VisualEffectError::Effect(id(BASE)))
    );
}
