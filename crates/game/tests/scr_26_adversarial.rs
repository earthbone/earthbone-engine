// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"REFR"), 1));
    plugin.push_top(Record::new(Tag::new(b"ACTI"), 2));
    plugin.push_top(Record::new(Tag::new(b"GLOB"), 3).zsub(Tag::new(b"EDID"), "GameHour").sub(Tag::new(b"FLTV"), 6.0f32.to_le_bytes()));
    let mut order = LoadOrder::new(); order.add("Camera.esm", plugin).unwrap(); Game::new(order)
}

fn global(game: &mut Game, name: &str, args: &[Value]) -> Value {
    let script = format!("Scr26{name}{}", game.vm.time().to_bits());
    let mut f = FunctionBuilder::new("Run", "Var").global().local("r", "Var");
    let mut op = vec![s("Game"), s(name), ident("r")];
    for (i, value) in args.iter().enumerate() { let p=format!("a{i}"); f=f.param(&p, if matches!(value, Value::Object(_)) {"ObjectReference"} else {"Var"}); op.push(ident(&p)); }
    let bytes=ScriptBuilder::new(&script, "").function(f.ins(CallStatic, op).ins(Return, vec![ident("r")]).build()).bytes();
    game.load_script(&bytes).unwrap(); let thread=game.vm.call_global(&script,"Run",args).unwrap(); assert!(game.run_until_idle(32)); game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_26_sun_position_is_finite_and_gaze_matches_reported_sun_direction() {
    let mut game=fixture();
    let x=global(&mut game,"GetSunPositionX",&[]).as_float();
    let y=global(&mut game,"GetSunPositionY",&[]).as_float();
    let z=global(&mut game,"GetSunPositionZ",&[]).as_float();
    assert!(x.is_finite() && y.is_finite() && z.is_finite());
    let player=game.world_mut().player(); let position=game.world().reference_ref(player).unwrap().position;
    game.world_mut().camera_look=[x-position[0],y-position[1],z-position[2]];
    assert_eq!(global(&mut game,"IsPlayerSungazing",&[]),Value::Bool(true));
    game.world_mut().camera_look=[position[0]-x,position[1]-y,position[2]-z];
    assert_eq!(global(&mut game,"IsPlayerSungazing",&[]),Value::Bool(false));
}

#[test]
fn scr_26_gaze_uses_direction_and_shake_rejects_bad_inputs_without_queueing() {
    let mut game=fixture(); game.world_mut().camera_look=[1.0,0.0,0.0];
    assert_eq!(global(&mut game,"IsPlayerSungazing",&[]),Value::Bool(true));
    game.world_mut().camera_look=[0.0,1.0,0.0];
    assert_eq!(global(&mut game,"IsPlayerSungazing",&[]),Value::Bool(false));
    global(&mut game,"ShakeCamera",&[Value::string("bad source"),Value::Float(0.5)]);
    assert!(game.world().camera_shakes.is_empty());
    let source=Value::Object(game.object_of_form(id(1)));
    global(&mut game,"ShakeCamera",&[source,Value::Float(0.25)]);
    assert_eq!(game.world().camera_shakes.len(),1);
    assert_eq!(game.world().camera_shakes[0].source,id(1));
    assert_eq!(game.world().camera_shakes[0].strength,0.25);
}
