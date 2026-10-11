// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn read_ref(game: &mut Game, name: &str) -> Value {
    let function = FunctionBuilder::new("Read", "Form")
        .global()
        .local("result", "Form")
        .ins(CallStatic, vec![s("Game"), s(name), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Hst18BreakerAdversarial", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Hst18BreakerAdversarial", "Read", &[])
        .unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn hst_18_clearing_crosshair_does_not_clear_grabbed_reference() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x123));
    plugin.push_top(Record::new(Tag::new(b"REFR"), 0x456));
    let mut order = LoadOrder::new();
    order.add("HostRefs.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let crosshair = GlobalFormId {
        file: 0,
        object: 0x123,
    };
    let grabbed = GlobalFormId {
        file: 0,
        object: 0x456,
    };
    let crosshair_object = game.object_of_form(crosshair);
    let grabbed_object = game.object_of_form(grabbed);

    game.world_mut().set_crosshair_reference(Some(crosshair));
    game.world_mut().set_grabbed_reference(Some(grabbed));
    assert_eq!(
        read_ref(&mut game, "GetCurrentCrosshairRef"),
        Value::Object(crosshair_object)
    );
    assert_eq!(
        read_ref(&mut game, "GetPlayerGrabbedRef"),
        Value::Object(grabbed_object)
    );

    game.world_mut().set_crosshair_reference(None);
    for _ in 0..3 {
        assert_eq!(read_ref(&mut game, "GetCurrentCrosshairRef"), Value::None);
        assert_eq!(
            read_ref(&mut game, "GetPlayerGrabbedRef"),
            Value::Object(grabbed_object)
        );
    }
}
