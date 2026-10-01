// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::Game;
use soe_papyrus::asm::{ident,s,FunctionBuilder,ScriptBuilder};
use soe_papyrus::{Opcode::*,Value};

fn id(object:u32)->GlobalFormId{GlobalFormId{file:0,object}}
fn call(game:&mut Game,name:&str)->Value{let script=format!("Scr31Adv{name}{}",game.vm.time().to_bits());let bytes=ScriptBuilder::new(&script,"").function(FunctionBuilder::new("Run","Var").global().local("r","Var").ins(CallStatic,vec![s("Game"),s(name),ident("r")]).ins(Return,vec![ident("r")]).build()).bytes();game.load_script(&bytes).unwrap();let t=game.vm.call_global(&script,"Run",&[]).unwrap();assert!(game.run_until_idle(32));game.vm.thread_result(t).unwrap()}

#[test]
fn scr_31_crosshair_and_grabbed_queries_follow_latest_transient_world_values(){
    let mut game=Game::new(LoadOrder::new());let player=game.world_mut().player();game.world_mut().set_crosshair_reference(Some(player));game.world_mut().set_grabbed_reference(Some(player));let cross=call(&mut game,"GetCurrentCrosshairRef").as_object().unwrap();let grabbed=call(&mut game,"GetPlayerGrabbedRef").as_object().unwrap();assert_eq!(game.world().form_of(cross),Some(player));assert_eq!(game.world().form_of(grabbed),Some(player));game.world_mut().set_crosshair_reference(None);game.world_mut().set_grabbed_reference(None);assert_eq!(call(&mut game,"GetCurrentCrosshairRef"),Value::None);assert_eq!(call(&mut game,"GetPlayerGrabbedRef"),Value::None);
}

#[test]
fn scr_31_title_sequence_preserves_supplied_string_without_adding_saved_state(){
    let mut game=Game::new(LoadOrder::new());let script="Scr31AdvTitle";let bytes=ScriptBuilder::new(script,"").function(FunctionBuilder::new("Run","None").global().param("name","String").ins(CallStatic,vec![s("Game"),s("StartTitleSequence"),ident("unused"),ident("name")]).build()).bytes();game.load_script(&bytes).unwrap();let t=game.vm.call_global(script,"Run",&[Value::string("")]).unwrap();assert!(game.run_until_idle(32));assert_eq!(game.vm.thread_result(t),Some(Value::None));assert_eq!(game.world().host_requests,[soe_game::world::HostRequest::StartTitleSequence(String::new())]);
}

#[test]
fn scr_31_sitting_rotation_preserves_signed_offset(){
    let mut game=Game::new(LoadOrder::new());let script="Scr31AdvSitting";let bytes=ScriptBuilder::new(script,"").function(FunctionBuilder::new("Run","None").global().param("degrees","Float").ins(CallStatic,vec![s("Game"),s("SetSittingRotation"),ident("unused"),ident("degrees")]).build()).bytes();game.load_script(&bytes).unwrap();let t=game.vm.call_global(script,"Run",&[Value::Float(-27.5)]).unwrap();assert!(game.run_until_idle(32));assert_eq!(game.vm.thread_result(t),Some(Value::None));assert_eq!(game.world().camera_controls.sitting_rotation_degrees,-27.5);
}
