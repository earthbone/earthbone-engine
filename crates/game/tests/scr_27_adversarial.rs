// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn global(game: &mut Game, name: &str) {
    let script=format!("Scr27{name}{}",game.vm.time().to_bits());
    let bytes=ScriptBuilder::new(&script,"").function(FunctionBuilder::new("Run","None").global().ins(CallStatic,vec![s("Game"),s(name),ident("unused")]).build()).bytes();
    game.load_script(&bytes).unwrap(); let thread=game.vm.call_global(&script,"Run",&[]).unwrap(); assert!(game.run_until_idle(32)); assert_eq!(game.vm.thread_result(thread),Some(Value::None));
}

#[test]
fn scr_27_clear_prison_clears_player_flags_and_saved_served_days_only() {
    let mut game=Game::new(LoadOrder::new()); let player=game.world_mut().player();
    { let mut world=game.world_mut(); let actor=world.actor(player).unwrap(); actor.arrested=true; actor.days_in_jail=7; }
    global(&mut game,"ClearPrison");
    { let mut world=game.world_mut(); let actor=world.actor(player).unwrap(); assert!(!actor.arrested); assert_eq!(actor.days_in_jail,0); actor.arrested=true; actor.days_in_jail=2; }
    let saved=game.save_state().unwrap(); let mut restored=Game::new(LoadOrder::new()); restored.load_state(&saved).unwrap();
    let saved_player=restored.world_mut().player(); let mut restored_world=restored.world_mut(); let actor=restored_world.actor(saved_player).unwrap();
    assert!(actor.arrested); assert_eq!(actor.days_in_jail,2);
}

#[test]
fn scr_27_serve_time_without_crime_faction_does_not_mutate_clock_or_jail_state() {
    let mut game=Game::new(LoadOrder::new()); let player=game.world_mut().player();
    let before=game.world().clock.clone();
    global(&mut game,"ServeTime");
    assert_eq!(game.world().clock,before); assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail,0);
}
