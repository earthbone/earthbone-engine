// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

fn call(game: &mut Game, method: &str, args: &[Value]) {
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("actor", "Form")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("actor"), ident("result")];
    for (index, _) in args.iter().enumerate() {
        let name = format!("arg{index}");
        function = function.param(&name, "Var");
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new("Scr11ExtraProbe", "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x801,
    }))];
    values.extend_from_slice(args);
    let thread = game
        .vm
        .call_global("Scr11ExtraProbe", "Run", &values)
        .unwrap();
    assert!(game.run_until_idle(32));
    assert!(game.vm.thread_result(thread).is_some());
}

#[test]
fn set_vehicle_rejects_extra_arguments_and_preserves_current_attachment() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x801, 0x802] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Scr11Extra.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let initial_vehicle = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x802,
    }));
    call(&mut game, "SetVehicle", &[initial_vehicle]);
    let actor_object = Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: 0x801,
    }));
    call(&mut game, "SetVehicle", &[actor_object, Value::Bool(true)]);
    assert_eq!(
        game.world_mut().actor_vehicle(GlobalFormId {
            file: 0,
            object: 0x801
        }),
        Some(GlobalFormId {
            file: 0,
            object: 0x802
        })
    );
    assert!(game
        .vm
        .log
        .contains("SetVehicle expects one ObjectReference argument"));
}
