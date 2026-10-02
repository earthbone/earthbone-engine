// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    world::{GameUiRequest, MenuRequest},
};
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn call(game: &mut Game, name: &str, args: &[Value]) {
    let script = format!("Scr30Adv{name}{}", game.vm.time().to_bits());
    let mut f = FunctionBuilder::new("Run", "None").global();
    let mut op = vec![s("Game"), s(name), ident("unused")];
    for (i, _) in args.iter().enumerate() {
        let p = format!("a{i}");
        f = f.param(&p, "Var");
        op.push(ident(&p));
    }
    let bytes = ScriptBuilder::new(&script, "")
        .function(f.ins(CallStatic, op).build())
        .bytes();
    game.load_script(&bytes).unwrap();
    let t = game.vm.call_global(&script, "Run", args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(t), Some(Value::None));
}

#[test]
fn scr_30_precache_requests_repeat_in_order_and_are_transient() {
    let mut game = Game::new(LoadOrder::new());
    for name in [
        "PrecacheCharGen",
        "PrecacheCharGenClear",
        "PrecacheCharGen",
        "PrecacheCharGenClear",
    ] {
        call(&mut game, name, &[]);
    }
    assert_eq!(
        game.world().menu_requests,
        [
            MenuRequest::PrecacheCharGen,
            MenuRequest::PrecacheCharGenClear,
            MenuRequest::PrecacheCharGen,
            MenuRequest::PrecacheCharGenClear
        ]
    );
    let saved = game.save_state().unwrap();
    let mut restored = Game::new(LoadOrder::new());
    restored.load_state(&saved).unwrap();
    assert!(restored.world().menu_requests.is_empty());
}

#[test]
fn scr_30_training_rejects_non_actor_and_keeps_other_ui_queues_empty() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Menus.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let object = Value::Object(game.object_of_form(id(0x800)));
    call(&mut game, "ShowTrainingMenu", &[object]);
    assert!(game.world().menu_requests.is_empty());
    assert_eq!(
        game.world_mut().take_game_ui_requests(),
        Vec::<GameUiRequest>::new()
    );
}

#[test]
fn scr_30_camera_target_rejects_non_actor_and_geometry_flag_can_toggle() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Menus.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.world_mut().player();
    let invalid = Value::Object(game.object_of_form(id(0x800)));
    call(&mut game, "SetCameraTarget", &[invalid]);
    assert_eq!(game.world().camera_controls.target, None);
    let valid = Value::Object(game.object_of_form(player));
    call(&mut game, "SetCameraTarget", &[valid]);
    assert_eq!(game.world().camera_controls.target, Some(player));
    call(&mut game, "ShowFirstPersonGeometry", &[Value::Bool(false)]);
    assert!(!game.world().camera_controls.first_person_geometry);
    call(&mut game, "ShowFirstPersonGeometry", &[Value::Bool(true)]);
    assert!(game.world().camera_controls.first_person_geometry);
}
