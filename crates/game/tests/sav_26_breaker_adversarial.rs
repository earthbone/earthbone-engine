// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial PCON and PARW live-record restoration checks.

use super::*;
use soe_game::world::ProjectileRecordExtra;
use soe_save::{reference::flags as change_flags, ChangeForm};

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

fn envelope_save() -> Save {
    let mut save = references::fixture();
    save.change_forms.extend([
        typed_change(0x400A02, 42, 0xA5A5_5A40, vec![0xCA, 0xFE, 0x01]),
        typed_change(0x400A03, 47, 0x5A5A_A5A5, vec![0xDE, 0xAD, 0xBE, 0xEF]),
    ]);
    save
}

#[test]
fn sav_26_type_mismatch_with_winning_record_rejects_without_mutation() {
    let mut save = references::fixture();
    save.change_forms
        .push(typed_change(0x400A02, 47, 0x1234, vec![1, 2, 3]));
    let mut world = World::new(references::fixture_order());
    let before = world.save_state();

    assert!(world.restore_vanilla_projectiles(&save).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.restored_projectile_extra(id(0, 0xA02)).is_none());
}

#[test]
fn sav_26_repeated_restore_rejects_without_replacing_saved_result() {
    let save = envelope_save();
    let mut world = World::new(references::fixture_order());
    assert_eq!(world.restore_vanilla_projectiles(&save).unwrap(), 2);
    let before = world.save_state();

    assert!(world.restore_vanilla_projectiles(&save).is_err());
    assert_eq!(world.save_state(), before);
    assert_eq!(
        world.restored_projectile_extra(id(0, 0xA02)),
        Some(&ProjectileRecordExtra {
            flags: 0xA5A5_5A40,
            payload: vec![0xCA, 0xFE, 0x01],
        })
    );
}

#[test]
fn sav_26_pcon_and_parw_queries_survive_combined_placement_save_reload() {
    let order = Arc::new(references::fixture_order());
    let mut world = World::from_shared_order(order.clone());
    let save = envelope_save();

    world.restore_vanilla_reference_placements(&save).unwrap();
    assert_eq!(world.restore_vanilla_projectiles(&save).unwrap(), 2);
    assert_eq!(
        world.reference_ref(id(0, 0xA02)).unwrap().base,
        Some(id(0, 0x900))
    );
    assert_eq!(
        world.reference(id(0, 0xA03)).unwrap().base,
        Some(id(0, 0x901))
    );

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    assert_eq!(
        loaded.restored_projectile_extra(id(0, 0xA02)),
        world.restored_projectile_extra(id(0, 0xA02))
    );
    assert_eq!(
        loaded.restored_projectile_extra(id(0, 0xA03)),
        world.restored_projectile_extra(id(0, 0xA03))
    );
    assert_eq!(
        loaded.reference_ref(id(0, 0xA02)).unwrap().base,
        Some(id(0, 0x900))
    );
}

#[test]
fn sav_26_truncated_pcon_move_prefix_rejects_reference_batch_atomically() {
    let mut save = references::fixture();
    let mut data = vec![];
    reference(0x400800).write(&mut data);
    data.extend(1.0_f32.to_le_bytes());
    save.change_forms
        .push(typed_change(0x400A02, 42, change_flags::MOVE, data));
    let mut world = World::new(references::fixture_order());
    let before = world.save_state();

    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.reference_ref(id(0, 0xA02)).is_none());
}

#[test]
fn sav_26_parw_placement_shaped_bytes_remain_opaque_to_reference_restore() {
    let mut save = references::fixture();
    let mut data = vec![];
    reference(0x400800).write(&mut data);
    for value in [81.0_f32, 82.0, 83.0, 0.1, 0.2, 0.3] {
        data.extend(value.to_le_bytes());
    }
    save.change_forms
        .push(typed_change(0x400A03, 47, change_flags::MOVE, data.clone()));

    let mut world = World::new(references::fixture_order());
    let report = world.restore_vanilla_reference_placements(&save).unwrap();
    assert!(!report.unplaced.contains(&id(0, 0xA03)));
    assert!(world.reference_ref(id(0, 0xA03)).is_none());
    assert_eq!(world.restore_vanilla_projectiles(&save).unwrap(), 1);
    assert_eq!(
        world.restored_projectile_extra(id(0, 0xA03)),
        Some(&ProjectileRecordExtra {
            flags: change_flags::MOVE,
            payload: data,
        })
    );
}
