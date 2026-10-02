// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const MUSIC_A: u32 = 0x801;
const MUSIC_B: u32 = 0x802;
const WRONG_FORM: u32 = 0x803;
const DELETED_MUSIC: u32 = 0x804;

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"MUSC"), MUSIC_A));
    plugin.push_top(Record::new(T(b"MUSC"), MUSIC_B));
    plugin.push_top(Record::new(T(b"STAT"), WRONG_FORM));
    plugin.push_top(
        Record::new(T(b"MUSC"), DELETED_MUSIC).with_flags(soe_esm::plugin::flags::DELETED),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, form: GlobalFormId, method: &str) -> Value {
    let name = format!("Scr37Probe{}", PROBES.fetch_add(1, Ordering::SeqCst));
    let fun = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![s(method), ident("target"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new(&name, "Form").function(fun).bytes();
    game.load_script(&bytes).unwrap();
    let target = Value::Object(game.object_of_form(form));
    let thread = game.vm.call_global(&name, "Run", &[target]).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_37_music_type_add_activates_the_receiver_once() {
    let mut game = game();
    assert_eq!(call(&mut game, id(MUSIC_A), "Add"), Value::None);
    assert_eq!(call(&mut game, id(MUSIC_A), "Add"), Value::None);
    assert_eq!(game.world().active_music_types(), [id(MUSIC_A)]);
}

#[test]
fn scr_37_music_type_add_preserves_addition_order() {
    let mut game = game();
    call(&mut game, id(MUSIC_A), "Add");
    call(&mut game, id(MUSIC_B), "Add");
    assert_eq!(
        game.world().active_music_types(),
        [id(MUSIC_A), id(MUSIC_B)]
    );
}

#[test]
fn scr_37_music_type_remove_deactivates_only_the_receiver() {
    let mut game = game();
    call(&mut game, id(MUSIC_A), "Add");
    call(&mut game, id(MUSIC_B), "Add");
    assert_eq!(call(&mut game, id(MUSIC_A), "Remove"), Value::None);
    assert_eq!(call(&mut game, id(MUSIC_A), "Remove"), Value::None);
    assert_eq!(game.world().active_music_types(), [id(MUSIC_B)]);
}

#[test]
fn scr_37_music_type_methods_log_wrong_form_errors_without_mutation() {
    let mut game = game();
    call(&mut game, id(MUSIC_A), "Add");
    call(&mut game, id(DELETED_MUSIC), "Add");
    call(&mut game, id(DELETED_MUSIC), "Remove");
    assert_eq!(game.world().active_music_types(), [id(MUSIC_A)]);
    let traces = game.traces();
    assert!(traces.iter().any(|trace| trace.contains("MusicType.Add")));
    assert!(
        traces
            .iter()
            .any(|trace| trace.contains("MusicType.Remove"))
    );
    assert!(game.world_mut().add_music_type(id(WRONG_FORM)).is_err());
    assert!(game.world_mut().remove_music_type(id(WRONG_FORM)).is_err());
    assert_eq!(game.world().active_music_types(), [id(MUSIC_A)]);
}
