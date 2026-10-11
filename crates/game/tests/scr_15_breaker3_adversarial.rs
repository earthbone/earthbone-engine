// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-15 adversarial check for the CK PlayIdleWithTarget contract.

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
    plugin.push_top(Record::new(T(b"ACHR"), 0x902).sub(T(b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"IDLE"), 0x901).zsub(T(b"EDID"), "TestPairedIdle"));
    let mut order = LoadOrder::new();
    order.add("SCR15Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn play_idle_with_target(game: &mut Game) -> Value {
    let probe = PROBES.fetch_add(1, Ordering::SeqCst);
    let class = format!("Scr15TargetProbe{probe}");
    let bytes = ScriptBuilder::new(&class, "Form")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("target", "Form")
                .param("idle", "Idle")
                .param("paired_target", "ObjectReference")
                .local("result", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("PlayIdleWithTarget"),
                        ident("target"),
                        ident("result"),
                        ident("idle"),
                        ident("paired_target"),
                    ],
                )
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let actor = Value::Object(game.object_of_form(id(0x900)));
    let idle = Value::Object(game.object_of_form(id(0x901)));
    let paired_target = Value::Object(game.object_of_form(id(0x902)));
    let thread = game
        .vm
        .call_global(&class, "Run", &[actor, idle, paired_target])
        .unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_15_paired_idle_is_dispatched_to_the_initiating_actor() {
    let mut game = game();
    assert_eq!(play_idle_with_target(&mut game), Value::Bool(true));
    assert_eq!(
        game.world().animation_requests,
        [soe_game::world::AnimationRequest {
            target: id(0x900),
            partner: Some(id(0x902)),
            animation: "TestPairedIdle".into(),
        }],
        "CK says the idle plays on this actor; the argument is its paired target"
    );
}
