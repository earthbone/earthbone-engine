// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Check opaque PCON and PARW payload boundaries through the runtime query.

use crate::vanilla_save::{id, reference, references};
use soe_game::{world::ProjectileRecordExtra, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::{CallStatic, Return}, Value};
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
        &ScriptBuilder::new("Sav26BoundaryProbe", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let argument = Value::Object(game.object_of_form(form));
    let thread = game
        .vm
        .call_global("Sav26BoundaryProbe", "Read", &[argument])
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
fn sav_26_runtime_query_round_trips_every_opaque_payload_byte_for_both_types() {
    let payload: Vec<u8> = (0..=u8::MAX).collect();
    let expected: String = payload
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .concat();
    let mut save = references::fixture();
    save.change_forms.extend([
        change(0x400A02, 42, payload.clone()),
        change(0x400A03, 47, payload.clone()),
    ]);
    let mut game = Game::new(references::fixture_order());
    game.world_mut().restore_vanilla_projectiles(&save).unwrap();

    assert_eq!(read_payload(&mut game, id(0, 0xA02)), expected);
    assert_eq!(read_payload(&mut game, id(0, 0xA03)), expected);

    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(read_payload(&mut loaded, id(0, 0xA02)), expected);
    assert_eq!(read_payload(&mut loaded, id(0, 0xA03)), expected);
}

#[test]
fn sav_26_empty_opaque_payload_remains_a_restored_record_after_engine_reload() {
    let mut save = references::fixture();
    save.change_forms.extend([
        change(0x400A02, 42, vec![]),
        change(0x400A03, 47, vec![]),
    ]);
    let mut game = Game::new(references::fixture_order());
    game.world_mut().restore_vanilla_projectiles(&save).unwrap();

    for form in [id(0, 0xA02), id(0, 0xA03)] {
        assert_eq!(read_payload(&mut game, form), "");
    }
    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();

    for form in [id(0, 0xA02), id(0, 0xA03)] {
        assert_eq!(
            loaded.world().restored_projectile_extra(form),
            Some(&ProjectileRecordExtra {
                flags: 0xA5A5_5A5A,
                payload: vec![],
            })
        );
        assert_eq!(read_payload(&mut loaded, form), "");
    }
}
