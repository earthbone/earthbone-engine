// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for SAV-26 PCON and PARW restoration.

use crate::vanilla_save::{id, reference, references};
use soe_game::{world::ProjectileRecordExtra, Game, World};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;
use soe_save::{reference::flags as change_flags, ChangeForm};

fn papyrus_projectile_payload(game: &mut Game, projectile: soe_esm::GlobalFormId) -> String {
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
        &ScriptBuilder::new("Sav26ProjectileProbe", "")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let form = Value::Object(game.object_of_form(projectile));
    let thread = game
        .vm
        .call_global("Sav26ProjectileProbe", "Read", &[form])
        .unwrap();
    assert!(game.run_until_idle(64));
    match game.vm.thread_result(thread).unwrap() {
        Value::Str(payload) => payload.to_string(),
        value => panic!("expected payload string, got {value:?}"),
    }
}

fn typed_change(raw: u32, type_index: u8, flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(raw),
        change_flags: flags,
        type_field: type_index,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_26_pcon_keeps_shared_move_prefix_and_opaque_suffix_separate() {
    let mut save = references::fixture();
    let mut data = vec![];
    reference(0x400800).write(&mut data);
    for value in [41.0_f32, 42.0, 43.0, 0.1, 0.2, 0.3] {
        data.extend(value.to_le_bytes());
    }
    data.extend([0xFA, 0xCE, 0x55]);
    save.change_forms
        .push(typed_change(0x400A02, 42, change_flags::MOVE, data.clone()));

    let header = save
        .change_forms
        .last()
        .unwrap()
        .reference_header(78)
        .unwrap()
        .unwrap();
    assert_eq!(
        header.initial,
        soe_save::reference::Initial::Moved(soe_save::reference::Placement {
            cell_or_world: reference(0x400800),
            position: [41.0, 42.0, 43.0],
            rotation: [0.1, 0.2, 0.3],
        })
    );
    assert_eq!(&data[header.bytes_consumed..], &[0xFA, 0xCE, 0x55]);

    let mut world = World::new(references::fixture_order());
    world.restore_vanilla_projectiles(&save).unwrap();
    assert_eq!(
        world.restored_projectile_extra(id(0, 0xA02)),
        Some(&ProjectileRecordExtra {
            flags: change_flags::MOVE,
            payload: data
        })
    );
}

#[test]
fn sav_26_parw_payload_that_looks_like_placement_stays_opaque() {
    let mut save = references::fixture();
    let mut data = vec![];
    reference(0x400800).write(&mut data);
    for value in [91.0_f32, 92.0, 93.0, 0.4, 0.5, 0.6] {
        data.extend(value.to_le_bytes());
    }
    save.change_forms
        .push(typed_change(0x400A03, 47, change_flags::MOVE, data.clone()));

    assert!(save
        .change_forms
        .last()
        .unwrap()
        .reference_header(78)
        .unwrap()
        .is_none());
    let order = references::fixture_order();
    let mut world = World::new(order);
    world.restore_vanilla_projectiles(&save).unwrap();
    assert_eq!(
        world.restored_projectile_extra(id(0, 0xA03)),
        Some(&ProjectileRecordExtra {
            flags: change_flags::MOVE,
            payload: data
        })
    );
}

#[test]
fn sav_26_projectile_change_rejects_a_same_id_nonmatching_winning_record() {
    let mut save = references::fixture();
    save.change_forms
        .push(typed_change(0x400900, 42, 0x1234, vec![1, 2]));
    let mut world = World::new(references::fixture_order());
    let before = world.save_state();

    assert!(world.restore_vanilla_projectiles(&save).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.restored_projectile_extra(id(0, 0xA02)).is_none());
}

#[test]
fn sav_26_duplicate_pcon_change_rejects_whole_batch() {
    let mut save = references::fixture();
    save.change_forms.extend([
        typed_change(0x400A02, 42, 0x1234, vec![1]),
        typed_change(0x400A02, 42, 0x5678, vec![2]),
    ]);
    let mut world = World::new(references::fixture_order());
    let before = world.save_state();

    assert!(world.restore_vanilla_projectiles(&save).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.restored_projectile_extra(id(0, 0xA02)).is_none());
}

#[test]
fn sav_26_parw_bytes_do_not_interrupt_pcon_placement_restore() {
    let mut save = references::fixture();
    let mut moved = vec![];
    reference(0x400800).write(&mut moved);
    for value in [1.0_f32, 2.0, 3.0, 0.0, 0.0, 0.0] {
        moved.extend(value.to_le_bytes());
    }
    save.change_forms.extend([
        typed_change(0x400A02, 42, change_flags::MOVE, moved),
        typed_change(0x400A03, 47, change_flags::MOVE, vec![0xAA]),
    ]);
    let mut world = World::new(references::fixture_order());

    world.restore_vanilla_reference_placements(&save).unwrap();
    assert_eq!(
        world.reference_ref(id(0, 0xA02)).unwrap().position,
        [1.0, 2.0, 3.0]
    );
    assert!(world.reference_ref(id(0, 0xA03)).is_none());
}

#[test]
fn sav_26_papyrus_native_reads_parw_payload_after_engine_reload() {
    let mut game = Game::new(references::fixture_order());
    let mut save = references::fixture();
    save.change_forms.push(typed_change(
        0x400A03,
        47,
        change_flags::MOVE,
        vec![0xB1, 0x00, 0xFE],
    ));
    game.world_mut().restore_vanilla_projectiles(&save).unwrap();
    let parw = id(0, 0xA03);

    assert_eq!(papyrus_projectile_payload(&mut game, parw), "b100fe");

    let bytes = game.save_state().unwrap();
    let mut loaded = Game::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(papyrus_projectile_payload(&mut loaded, parw), "b100fe");
}
