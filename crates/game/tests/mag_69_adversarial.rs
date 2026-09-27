// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::CallMethod;
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn condition(function: u16, parameter: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[4..8].copy_from_slice(&2.0_f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&function.to_le_bytes());
    bytes[12..16].copy_from_slice(&parameter.to_le_bytes());
    bytes
}

fn case(function: u16, parameter: u32, variable: &str, class: &str, quest: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x500).sub(Tag::new(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(Tag::new(b"ACHR"), 0x100).sub(Tag::new(b"NAME"), 0x500_u32.to_le_bytes()).sub(Tag::new(b"DATA"), vec![0; 24]));
    if quest { plugin.push_top(Record::new(Tag::new(b"QUST"), 0x900)); }
    let mut mgef = vec![0; 152];
    mgef[0..4].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    mgef[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    mgef[64..68].copy_from_slice(&0_u32.to_le_bytes());
    mgef[68..72].copy_from_slice(&24_u32.to_le_bytes());
    mgef[80..84].copy_from_slice(&1_u32.to_le_bytes());
    mgef[84..88].copy_from_slice(&3_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200)
        .sub(Tag::new(b"DATA"), mgef)
        .sub(Tag::new(b"CTDA"), condition(function, parameter))
        .zsub(Tag::new(b"CIS2"), variable));
    let mut spell_data = vec![0; 36];
    spell_data[16..20].copy_from_slice(&1_u32.to_le_bytes());
    spell_data[20..24].copy_from_slice(&3_u32.to_le_bytes());
    let mut efit = 10.0_f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0_u32.to_le_bytes());
    efit.extend_from_slice(&0_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"SPEL"), 0x300)
        .sub(Tag::new(b"SPIT"), spell_data).sub(Tag::new(b"EFID"), 0x200_u32.to_le_bytes()).sub(Tag::new(b"EFIT"), efit));
    let mut order = LoadOrder::new();
    order.add("Variable.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().set_actor_value(id(0x100), "health", 100.0).unwrap();
    let mut script = ScriptBuilder::new("Gate", class).variable(variable, "Int", i(2)).build();
    script.user_flags.push(soe_papyrus::pex::UserFlag { name: "conditional".into(), bit: 3 });
    script.objects[0].user_flags = 8;
    script.objects[0].variables[0].user_flags = 8;
    game.vm.load(script);
    let owner = game.object_of_form(id(parameter));
    game.vm.attach(owner, "Gate").unwrap();
    game
}

fn native(game: &mut Game, name: &str) {
    let mut function = FunctionBuilder::new("Run", "None").global()
        .param("spell", "Spell").param("source", "ObjectReference");
    let mut operands = vec![s(name), ident("spell"), ident("unused"), ident("source")];
    if name == "RemoteCast" {
        function = function.param("caster", "Actor");
        operands.push(ident("caster"));
    }
    function = function.param("target", "Actor");
    operands.push(ident("target"));
    let script = ScriptBuilder::new("CallMagic", "")
        .function(function.ins(CallMethod, operands).build()).bytes();
    game.load_script(&script).unwrap();
    let spell = Value::Object(game.object_of_form(id(0x300)));
    let source = Value::Object(game.object_of_form(id(0x100)));
    let caster = Value::Object(game.object_of_form(id(0x100)));
    let target = Value::Object(game.object_of_form(id(0x100)));
    let args = if name == "Cast" { vec![spell, source, target] } else { vec![spell, source, caster, target] };
    let thread = game.vm.call_global("CallMagic", "Run", &args).unwrap();
    assert!(game.run_until_idle(64));
    assert!(matches!(game.vm.thread_result(thread).unwrap(), Value::None));
}

#[test]
fn mag_69_live_quest_variable_conditions_cover_cast_and_remote_cast() {
    for native_name in ["Cast", "RemoteCast"] {
        let mut game = case(629, 0x900, "::gate", "Quest", true);
        native(&mut game, native_name);
        assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 90.0);
        let quest = game.object_of_form(id(0x900));
        game.vm.set_var(quest, "Gate", "::gate", Value::Int(1)).unwrap();
        native(&mut game, native_name);
        assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 90.0);
        assert_eq!(game.world().active_effects().count(), 1);
    }
}

#[test]
fn mag_69_live_reference_variable_conditions_cover_cast_and_remote_cast() {
    for native_name in ["Cast", "RemoteCast"] {
        let mut game = case(630, 0x100, "gate", "Actor", false);
        native(&mut game, native_name);
        assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 90.0);
        let actor = game.object_of_form(id(0x100));
        game.vm.set_var(actor, "Gate", "gate", Value::Int(1)).unwrap();
        native(&mut game, native_name);
        assert_eq!(game.world_mut().actor_value(id(0x100), "health"), 90.0);
        assert_eq!(game.world().active_effects().count(), 1);
    }
}
