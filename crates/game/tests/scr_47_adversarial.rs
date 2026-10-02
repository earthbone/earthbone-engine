// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::{CallMethod, Return},
    Value,
};
fn id(n: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object: n }
}
fn call(g: &mut Game, name: &str, form: GlobalFormId) {
    let script = format!("Scr47Adv{name}");
    let bytes = ScriptBuilder::new(&script, "Spell")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("spell", "Spell")
                .local("r", "None")
                .ins(CallMethod, vec![s(name), ident("spell"), ident("r")])
                .ins(Return, vec![])
                .build(),
        )
        .bytes();
    g.load_script(&bytes).unwrap();
    let spell = Value::Object(g.object_of_form(form));
    let t = g.vm.call_global(&script, "Run", &[spell]).unwrap();
    assert!(g.run_until_idle(32));
    assert_eq!(g.vm.thread_result(t), Some(Value::None));
}
#[test]
fn scr_47_art_methods_validate_spell_and_queue_each_action() {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"SPEL"), 0x800).sub(Tag(*b"SPIT"), vec![0; 36]));
    let mut order = LoadOrder::new();
    order.add("MissingSpell.esm", p).unwrap();
    let mut g = Game::new(order);
    let before = g.world().spell_casts.clone();
    call(&mut g, "Preload", id(0x800));
    call(&mut g, "Unload", id(0x800));
    assert_eq!(g.world().spell_casts, before);
    assert_eq!(g.world().active_effects().count(), 0);
    assert_eq!(
        g.world_mut().take_spell_art_requests(),
        vec![
            soe_game::world::SpellArtRequest {
                spell: id(0x800),
                action: soe_game::world::SpellArtAction::Preload,
            },
            soe_game::world::SpellArtRequest {
                spell: id(0x800),
                action: soe_game::world::SpellArtAction::Unload,
            },
        ]
    );
}

#[test]
fn scr_47_invalid_spell_art_requests_log_errors_without_queueing() {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(Tag(*b"SPEL"), 0x800).sub(Tag(*b"SPIT"), vec![0; 1]));
    let mut order = LoadOrder::new();
    order.add("MissingSpell.esm", p).unwrap();
    let mut g = Game::new(order);
    call(&mut g, "Preload", id(0x800));
    call(&mut g, "Unload", id(0x800));
    assert!(g.world_mut().take_spell_art_requests().is_empty());
    let traces = g.traces();
    assert!(
        traces
            .iter()
            .any(|line| line.contains("Spell.Preload: invalid magic record")),
        "{traces:?}"
    );
    assert!(
        traces
            .iter()
            .any(|line| line.contains("Spell.Unload: invalid magic record")),
        "{traces:?}"
    );
}
