// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const PLAYER: u32 = 0x14;
const TARGET: u32 = 0x801;
const PLAYER_BASE: u32 = 0x800;
const TARGET_BASE: u32 = 0x802;
const WARM_KEYWORD: u32 = 0x803;
const WARM_ARMOR: u32 = 0x804;
const BROKEN_ARMOR: u32 = 0x805;
const QUEST: u32 = 0x806;
const PERK: u32 = 0x807;
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameter: u32, expected: f32) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 0];
    bytes.extend(expected.to_le_bytes());
    bytes.extend(function.to_le_bytes());
    bytes.extend([0, 0]);
    bytes.extend(parameter.to_le_bytes());
    bytes.extend([0; 12]);
    bytes.extend((-1_i32).to_le_bytes());
    bytes
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), PLAYER_BASE));
    plugin.push_top(Record::new(T(b"NPC_"), TARGET_BASE));
    plugin.push_top(Record::new(T(b"ACHR"), PLAYER).sub(T(b"NAME"), PLAYER_BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"ACHR"), TARGET).sub(T(b"NAME"), TARGET_BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"KYWD"), WARM_KEYWORD).zsub(T(b"EDID"), "Survival_ArmorWarm"));
    plugin.push_top(
        Record::new(T(b"ARMO"), WARM_ARMOR)
            .zsub(T(b"EDID"), "WarmArmor")
            .sub(T(b"KWDA"), WARM_KEYWORD.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ARMO"), BROKEN_ARMOR)
            .zsub(T(b"EDID"), "BrokenArmor")
            .sub(T(b"KWDA"), [1, 2]),
    );
    plugin.push_top(Record::new(T(b"QUST"), QUEST).zsub(T(b"EDID"), "GateQuest"));
    plugin.push_top(
        Record::new(T(b"PERK"), PERK)
            .sub(T(b"PRKE"), [2, 0, 0])
            .sub(T(b"DATA"), [43, 2, 2])
            .sub(T(b"PRKC"), [0])
            .sub(T(b"CTDA"), condition(629, QUEST, 1.0))
            .zsub(T(b"CIS2"), "::gate")
            .sub(T(b"PRKC"), [1])
            .sub(T(b"EPFT"), [1])
            .sub(T(b"EPFD"), 25.0_f32.to_le_bytes())
            .sub(T(b"PRKF"), []),
    );
    let mut order = LoadOrder::new();
    order.add("Scr17Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, receiver: u32, method: &str, args: &[Value], types: &[&str], returns: &str) -> Value {
    let script_name = format!("Scr17BreakerProbe{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", returns)
        .global()
        .param("actor", "Actor")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("actor"), ident("result")];
    for (n, ty) in types.iter().enumerate() {
        let name = format!("arg{n}");
        function = function.param(&name, ty);
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new(&script_name, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(id(receiver)))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_17_intimidation_uses_live_conditional_perk_variables() {
    let mut game = game();
    let quest_object = game.object_of_form(id(QUEST));
    let mut script = ScriptBuilder::new("Scr17Gate", "Quest")
        .auto_property("Gate", "Int", i(1))
        .build();
    script.user_flags.push(soe_papyrus::pex::UserFlag {
        name: "conditional".into(),
        bit: 3,
    });
    script.objects[0].user_flags = 8;
    script.objects[0].variables[0].user_flags = 8;
    game.vm.load(script);
    game.vm.attach(quest_object, "Scr17Gate").unwrap();
    game.vm.set_var(quest_object, "Scr17Gate", "::Gate", Value::Int(1)).unwrap();
    game.world_mut().actor(id(PLAYER)).unwrap().perks.insert(id(PERK));
    game.world_mut().set_actor_value(id(PLAYER), "speechcraft", 49.0).unwrap();
    game.world_mut().set_actor_value(id(TARGET), "confidence", 2.0).unwrap();
    assert_eq!(call(&mut game, TARGET, "WillIntimidateSucceed", &[], &[], "Bool"), Value::Bool(true));
}

#[test]
fn scr_17_warmth_rejects_malformed_keyword_arrays() {
    let mut game = game();
    game.world_mut().actor(id(TARGET)).unwrap().equipped.insert(id(BROKEN_ARMOR));
    assert_eq!(call(&mut game, TARGET, "GetWarmthRating", &[], &[], "Float"), Value::Float(0.0));
    assert!(game.vm.log.messages().iter().any(|message| message.to_string().contains("GetWarmthRating")));
}

#[test]
fn scr_17_package_move_rejects_nonfinite_centers_without_moving() {
    let mut game = game();
    let original = game.world_mut().reference(id(TARGET)).unwrap().position;
    game.world_mut().actor(id(TARGET)).unwrap().ai.center = Some([f32::NAN, 4.0, 5.0]);
    assert_eq!(call(&mut game, TARGET, "MoveToPackageLocation", &[], &[], "None"), Value::None);
    assert_eq!(game.world_mut().reference(id(TARGET)).unwrap().position, original);
}
