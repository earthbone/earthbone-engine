// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-15 adversarial Actor native checks against the CK contracts.

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
    let mut spell_data = vec![0_u8; 36];
    spell_data[8..12].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x902)
            .zsub(T(b"EDID"), "ProbePower")
            .sub(T(b"SPIT"), spell_data),
    );
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
    let class = format!("Scr15BreakerProbe{probe}");
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
fn scr_15_get_equipped_spell_reads_other_and_instant_sources() {
    let mut game = game();
    let actor = Value::Object(game.object_of_form(id(0x900)));
    let spell = Value::Object(game.object_of_form(id(0x902)));
    for source in [2, 3] {
        call(
            &mut game,
            "EquipSpell",
            &[spell.clone(), Value::Int(source)],
            "Var",
        );
        assert_eq!(
            call(&mut game, "GetEquippedSpell", &[Value::Int(source)], "Form",),
            spell,
            "CK defines source {source} as a readable spell slot for {actor:?}"
        );
    }
}

#[test]
fn scr_15_voice_recovery_counts_down_and_rejects_invalid_replacement() {
    let mut game = game();
    assert!(game.world_mut().set_voice_recovery_time(id(0x900), 7.5));
    game.tick(2.5);
    assert_eq!(
        call(&mut game, "GetVoiceRecoveryTime", &[], "Float"),
        Value::Float(5.0)
    );
    assert!(!game
        .world_mut()
        .set_voice_recovery_time(id(0x900), f32::NAN));
    assert_eq!(
        call(&mut game, "GetVoiceRecoveryTime", &[], "Float"),
        Value::Float(5.0)
    );
}

#[test]
fn scr_15_trespass_native_state_survives_engine_save_load() {
    let mut game = game();
    assert_eq!(call(&mut game, "Trespass", &[], "Var"), Value::None);
    let saved = game.save_state().unwrap();
    game.world_mut().actor(id(0x900)).unwrap().trespassing = false;
    game.load_state(&saved).unwrap();
    assert!(game.world().actor_ref(id(0x900)).unwrap().trespassing);
}
