// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn call(game: &mut Game, method: &str, args: &[Value], result_type: &str) -> Value {
    let mut function = FunctionBuilder::new("Run", result_type).param("target", "Form");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    for (index, _) in args.iter().enumerate() {
        let name = format!("arg{index}");
        function = function.param(&name, "Var");
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new("Scr15ExtraProbe", "Form")
        .function(
            function
                .local("result", result_type)
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let actor = Value::Object(game.object_of_form(id(0x900)));
    let mut values = vec![actor];
    values.extend_from_slice(args);
    let thread = game
        .vm
        .call_global("Scr15ExtraProbe", "Run", &values)
        .unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn equipped_spell_reads_other_and_instant_sources_after_assignment() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        0x800,
        "ProbeBase",
        "ProbeBase",
        npc::Config::default(),
        npc::Stats::default(),
    ));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut spell_data = vec![0_u8; 36];
    spell_data[8..12].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), 0x902)
            .zsub(Tag(*b"EDID"), "ProbePower")
            .sub(Tag(*b"SPIT"), spell_data),
    );
    let mut order = LoadOrder::new();
    order.add("Scr15Extra.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let spell = Value::Object(game.object_of_form(id(0x902)));
    for source in [2, 3] {
        call(
            &mut game,
            "EquipSpell",
            &[spell.clone(), Value::Int(source)],
            "Var",
        );
        assert_eq!(
            call(&mut game, "GetEquippedSpell", &[Value::Int(source)], "Form"),
            spell,
            "CK defines {source} as a valid equipped spell source"
        );
    }
}
