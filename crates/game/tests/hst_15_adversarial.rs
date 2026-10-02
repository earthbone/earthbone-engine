// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    Opcode::CallMethod,
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident},
};

const CART: u32 = 0x900;
const HORSE: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(object: u32, position: [f32; 3]) -> Record {
    let data = position
        .into_iter()
        .chain([0.0_f32; 3])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

#[test]
fn hst_15_tether_native_positions_horse_at_cart_on_call() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(placed(CART, [0.0, 0.0, 0.0]));
    plugin.push_top(placed(HORSE, [100.0, 100.0, 0.0]));
    let mut order = LoadOrder::new();
    order.add("Tether.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let before = game.world_mut().reference(id(HORSE)).unwrap().position;
    let class = "Hst15Probe";
    let script = ScriptBuilder::new(class, "ObjectReference")
        .function(
            FunctionBuilder::new("Run", "None")
                .param("horse", "ObjectReference")
                .local("result", "None")
                .ins(
                    CallMethod,
                    vec![
                        ident("TetherToHorse"),
                        ident("self"),
                        ident("result"),
                        ident("horse"),
                    ],
                )
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let cart = game.object_of_form(id(CART));
    let horse = game.object_of_form(id(HORSE));
    game.vm.attach(cart, class).unwrap();
    game.vm.call(cart, "Run", &[Value::Object(horse)]).unwrap();
    assert!(game.run_until_idle(32));

    let after = game.world().reference_ref(id(HORSE)).unwrap().position;
    assert_ne!(
        after, before,
        "CK TetherToHorse moves the horse into cart position"
    );
}
