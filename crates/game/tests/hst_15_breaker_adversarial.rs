// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{world::ReferencePhysicsRequest, Game};
use soe_papyrus::{
    asm::{ident, FunctionBuilder, ScriptBuilder},
    Opcode::CallMethod,
    Value,
};

const CART: u32 = 0x900;
const HORSE: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placed(object: u32, position: [f32; 3], rotation: [f32; 3]) -> Record {
    let data = position
        .into_iter()
        .chain(rotation)
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    Record::new(Tag(*b"REFR"), object)
        .sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

fn game_with_horse(horse: bool, cart_position: [f32; 3], yaw: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    plugin.push_top(placed(CART, cart_position, [0.0, 0.0, yaw]));
    if horse {
        plugin.push_top(placed(HORSE, [100.0, 100.0, 0.0], [0.0; 3]));
    }
    let mut order = LoadOrder::new();
    order.add("Tether.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let class = "Hst15BreakerProbe";
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
    game.vm.attach(cart, class).unwrap();
    let horse = game.object_of_form(id(HORSE));
    game.vm.call(cart, "Run", &[Value::Object(horse)]).unwrap();
    assert!(game.run_until_idle(32));
    game
}

#[test]
fn hst_15_horse_position_uses_ck_authored_offsets() {
    let game = game_with_horse(true, [20.0, 40.0, 5.0], 0.0);
    let actual = game.world().reference_ref(id(HORSE)).unwrap().position;

    assert_eq!(actual, [92.0, 340.0, 5.0]);
}

#[test]
fn hst_15_missing_horse_does_not_queue_a_host_constraint() {
    let mut game = game_with_horse(false, [0.0; 3], 0.0);
    let requests = game.world_mut().take_reference_physics_requests();

    assert_eq!(requests, Vec::<ReferencePhysicsRequest>::new());
}

#[test]
fn hst_15_ck_horse_offsets_rotate_with_the_cart() {
    let game = game_with_horse(true, [0.0; 3], std::f32::consts::FRAC_PI_2);
    let actual = game.world().reference_ref(id(HORSE)).unwrap().position;

    assert!(
        (actual[0] + 300.0).abs() < 0.01 && (actual[1] - 72.0).abs() < 0.01,
        "CK offsets rotated by the cart yaw should be [-300, 72, 0], got {actual:?}"
    );
    assert_eq!(actual[2], 0.0);
}

#[test]
fn hst_15_self_reference_does_not_move_or_tether_the_cart() {
    let mut game = game_with_horse(true, [20.0, 40.0, 5.0], 0.0);
    let cart_id = id(CART);
    let cart = game.object_of_form(cart_id);
    game.world_mut().take_reference_physics_requests();
    let before = game.world().reference_ref(cart_id).unwrap().position;

    game.vm.call(cart, "Run", &[Value::Object(cart)]).unwrap();
    assert!(game.run_until_idle(32));

    assert_eq!(game.world().reference_ref(cart_id).unwrap().position, before);
    assert!(game
        .world_mut()
        .take_reference_physics_requests()
        .is_empty());
}
