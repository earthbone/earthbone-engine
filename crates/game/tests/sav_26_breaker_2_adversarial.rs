// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check the Papyrus view of opaque PCON and PARW bytes.

use crate::vanilla_save::{id, reference, references};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{
    Opcode::{CallStatic, Return},
    Value,
};
use soe_save::ChangeForm;

fn read_payload(game: &mut Game, form: soe_esm::GlobalFormId) -> String {
    let function = FunctionBuilder::new("Read", "String")
        .global()
        .param("projectile", "Form")
        .local("payload", "String")
        .ins(
            CallStatic,
            vec![
                s("Game"),
                s("GetSavedProjectilePayload"),
                ident("payload"),
                ident("projectile"),
            ],
        )
        .ins(Return, vec![ident("payload")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Sav26BreakerProbe", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let argument = Value::Object(game.object_of_form(form));
    let thread = game
        .vm
        .call_global("Sav26BreakerProbe", "Read", &[argument])
        .unwrap();
    assert!(game.run_until_idle(64));
    match game.vm.thread_result(thread).unwrap() {
        Value::Str(payload) => payload.to_string(),
        value => panic!("expected payload string, got {value:?}"),
    }
}

fn change(raw: u32, type_index: u8, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(raw),
        change_flags: 0xA5A5_5A5A,
        type_field: type_index,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_26_papyrus_payload_hex_preserves_opaque_bytes_for_both_types() {
    let mut save = references::fixture();
    let payload = vec![0x00, 0x0f, 0x10, 0xab, 0xcd, 0xef];
    save.change_forms.extend([
        change(0x400A02, 42, payload.clone()),
        change(0x400A03, 47, payload),
    ]);
    let mut game = Game::new(references::fixture_order());
    game.world_mut().restore_vanilla_projectiles(&save).unwrap();

    assert_eq!(read_payload(&mut game, id(0, 0xA02)), "000f10abcdef");
    assert_eq!(read_payload(&mut game, id(0, 0xA03)), "000f10abcdef");

    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(read_payload(&mut loaded, id(0, 0xA02)), "000f10abcdef");
    assert_eq!(read_payload(&mut loaded, id(0, 0xA03)), "000f10abcdef");
}

#[test]
fn sav_26_papyrus_payload_query_returns_empty_for_unrestored_and_nonprojectile_forms() {
    let mut save = references::fixture();
    save.change_forms
        .push(change(0x400A03, 47, vec![0x80, 0x00]));
    let mut game = Game::new(references::fixture_order());
    game.world_mut().restore_vanilla_projectiles(&save).unwrap();

    assert_eq!(read_payload(&mut game, id(0, 0xA02)), "");
    assert_eq!(read_payload(&mut game, id(0, 0x900)), "");
}
