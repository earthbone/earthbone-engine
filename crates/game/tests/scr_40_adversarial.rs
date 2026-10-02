// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-40 follower-favor and destruction native contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::{
    Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};
use std::sync::atomic::{AtomicUsize, Ordering};

const BASE: u32 = 0x100;
const REF: u32 = 0x200;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut dest = 100i32.to_le_bytes().to_vec();
    dest.extend_from_slice(&[2, 0, 0, 0]);
    let mut stage_one = vec![75, 0, 1, 0];
    stage_one.extend_from_slice(&[0; 16]);
    let mut stage_two = vec![25, 0, 1, 0];
    stage_two.extend_from_slice(&[0; 16]);
    plugin.push_top(
        Record::new(T(b"ACTI"), BASE)
            .sub(T(b"DEST"), dest)
            .sub(T(b"DSTD"), stage_one)
            .sub(T(b"DMDL"), b"meshes/damaged.nif\0".to_vec())
            .sub(T(b"DSTF"), vec![])
            .sub(T(b"DSTD"), stage_two)
            .sub(T(b"DMDL"), b"meshes/destroyed.nif\0".to_vec())
            .sub(T(b"DSTF"), vec![]),
    );
    plugin.push_top(Record::new(T(b"REFR"), REF).sub(T(b"NAME"), BASE.to_le_bytes().to_vec()));
    let mut order = LoadOrder::new();
    order.add("Scr40.esm", plugin).unwrap();
    Game::new(order)
}

fn native(game: &mut Game, method: &str, args: &[Value]) {
    static NEXT_CALL: AtomicUsize = AtomicUsize::new(0);
    let class = format!(
        "Invoke{method}{}",
        NEXT_CALL.fetch_add(1, Ordering::Relaxed)
    );
    let mut function =
        FunctionBuilder::new(&format!("Run{method}"), "None").local("result", "None");
    let mut code = vec![s(method), ident("self"), ident("result")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Var");
        code.push(ident(&name));
    }
    game.load_script(
        &ScriptBuilder::new(&class, "ObjectReference")
            .function(function.ins(soe_papyrus::Opcode::CallMethod, code).build())
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(id(REF));
    game.vm.attach(object, &class).unwrap();
    let thread = game.vm.call(object, &format!("Run{method}"), args).unwrap();
    assert!(game.run_until_idle(16));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn damage(game: &mut Game, amount: f32) {
    native(game, "DamageObject", &[Value::Float(amount)]);
}

#[test]
fn scr_40_set_no_favor_allowed_native_uses_boolean_argument() {
    let mut game = fixture();
    assert!(game.world_mut().set_no_favor_allowed(id(REF), true));
    native(&mut game, "SetNoFavorAllowed", &[Value::Bool(false)]);
    assert!(
        !game
            .world_mut()
            .reference(id(REF))
            .unwrap()
            .no_favor_allowed
    );
}

#[test]
fn scr_40_set_no_favor_allowed_is_saved_and_missing_references_are_rejected_by_api() {
    let mut game = fixture();
    native(&mut game, "SetNoFavorAllowed", &[Value::Bool(true)]);
    let save = game.world().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&save).unwrap();
    assert!(
        restored
            .world_mut()
            .reference(id(REF))
            .unwrap()
            .no_favor_allowed
    );
    assert!(!restored.world_mut().set_no_favor_allowed(id(0xdead), true));
}

#[test]
fn scr_40_damage_object_uses_argument_and_emits_crossed_stage_events() {
    let mut game = fixture();
    assert_eq!(game.world_mut().base_of(id(REF)), Some(id(BASE)));
    damage(&mut game, 30.0);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), 0);
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    game.world_mut().damage_object(id(REF), 50.0, &mut allocate);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), 1);
    assert!(game.world().pending.iter().any(|pending| matches!(
        pending,
        Pending::Event { name, args, .. }
            if name == "OnDestructionStageChanged" && args == &[Value::Int(1)]
    )));
}

#[test]
fn scr_40_clear_destruction_resets_effects_without_events_or_reapplication() {
    let game = fixture();
    let _ = game.world_mut().reference(id(REF)).unwrap();
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    game.world_mut().damage_object(id(REF), 80.0, &mut allocate);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), 1);
    let pending = game.world().pending.len();

    game.world_mut().clear_destruction(id(REF), &mut allocate);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), -1);
    let mut world = game.world_mut();
    let state = world.reference(id(REF)).unwrap();
    assert!(!state.destroyed && state.enabled);
    drop(world);
    assert_eq!(game.world().pending.len(), pending);

    game.world_mut()
        .damage_object(id(REF), 100.0, &mut allocate);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), -1);
    assert_eq!(game.world().pending.len(), pending);
}

#[test]
fn scr_40_clear_destruction_native_resets_destroyed_reference() {
    let mut game = fixture();
    native(&mut game, "DamageObject", &[Value::Float(80.0)]);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), 1);

    native(&mut game, "ClearDestruction", &[]);
    assert_eq!(game.world_mut().destruction_stage(id(REF)), -1);
    let mut world = game.world_mut();
    let state = world.reference(id(REF)).unwrap();
    assert!(!state.destroyed && state.enabled);
}

#[test]
fn scr_40_clear_destruction_marker_survives_save_round_trip() {
    let game = fixture();
    let _ = game.world_mut().reference(id(REF)).unwrap();
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    game.world_mut().damage_object(id(REF), 80.0, &mut allocate);
    game.world_mut().clear_destruction(id(REF), &mut allocate);
    let save = game.world().save_state();
    let restored = fixture();
    restored.world_mut().load_state(&save).unwrap();
    let mut allocate = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    restored
        .world_mut()
        .damage_object(id(REF), 100.0, &mut allocate);
    assert_eq!(restored.world_mut().destruction_stage(id(REF)), -1);
}
