// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn read_ref(game: &mut Game, script: &str, name: &str) -> Value {
    let function = FunctionBuilder::new("Read", "Form")
        .global()
        .local("result", "Form")
        .ins(CallStatic, vec![s("Game"), s(name), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(&ScriptBuilder::new(script, "").function(function).bytes())
        .unwrap();
    let thread = game.vm.call_global(script, "Read", &[]).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn hst_18_crosshair_and_grabbed_refs_are_transient_across_world_load() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x123));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x456));
    let mut order = LoadOrder::new();
    order.add("Hst18Transient.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().set_crosshair_reference(Some(GlobalFormId {
        file: 0,
        object: 0x123,
    }));
    game.world_mut().set_grabbed_reference(Some(GlobalFormId {
        file: 0,
        object: 0x456,
    }));

    let saved = game.world().save_state();
    let mut restored = Game::new(LoadOrder::new());
    restored.world_mut().load_state(&saved).unwrap();
    for _ in 0..3 {
        assert_eq!(
            read_ref(&mut restored, "Hst18Transient", "GetCurrentCrosshairRef"),
            Value::None
        );
        assert_eq!(
            read_ref(&mut restored, "Hst18Transient", "GetPlayerGrabbedRef"),
            Value::None
        );
    }
}
