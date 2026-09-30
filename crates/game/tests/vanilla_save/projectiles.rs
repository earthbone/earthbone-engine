// SPDX-License-Identifier: GPL-3.0-or-later

//! PCON and PARW restore their shared saved placement into the reference table.

use super::*;
use soe_save::{reference::flags as change_flags, ChangeForm};

fn moved_projectile(raw: u32, type_index: u8, position: [f32; 3]) -> ChangeForm {
    let mut data = vec![];
    reference(0x400800).write(&mut data);
    for value in position.into_iter().chain([0.0, 0.25, 0.0]) {
        data.extend(value.to_le_bytes());
    }
    ChangeForm {
        form: reference(raw),
        change_flags: change_flags::MOVE,
        type_field: type_index,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

fn projectile_save() -> Save {
    let mut save = references::fixture();
    save.change_forms
        .push(moved_projectile(0x400A02, 42, [11.0, 12.0, 13.0]));
    save.change_forms
        .push(moved_projectile(0x400A03, 47, [21.0, 22.0, 23.0]));
    save
}

#[test]
fn sav_26_pcon_parw_changes_restore_runtime_placement_and_survive_engine_save_load() {
    let mut world = World::new(references::fixture_order());
    let report = world
        .restore_vanilla_reference_placements(&projectile_save())
        .unwrap();

    assert_eq!(report.references, 5);
    assert_eq!(
        world.reference_ref(id(0, 0xA02)).unwrap().position,
        [11.0, 12.0, 13.0]
    );
    assert_eq!(
        world.reference_ref(id(0, 0xA03)).unwrap().position,
        [21.0, 22.0, 23.0]
    );
    assert_eq!(
        world.reference(id(0, 0xA02)).unwrap().base,
        Some(id(0, 0x900))
    );
    assert_eq!(
        world.reference(id(0, 0xA03)).unwrap().base,
        Some(id(0, 0x901))
    );

    let bytes = world.save_state();
    let mut restored = World::new(references::fixture_order());
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.reference_ref(id(0, 0xA02)).unwrap().position,
        [11.0, 12.0, 13.0]
    );
    assert_eq!(
        restored.reference_ref(id(0, 0xA03)).unwrap().position,
        [21.0, 22.0, 23.0]
    );
    assert_eq!(
        restored.reference(id(0, 0xA02)).unwrap().base,
        Some(id(0, 0x900))
    );
    assert_eq!(
        restored.reference(id(0, 0xA03)).unwrap().base,
        Some(id(0, 0x901))
    );
}

#[test]
fn sav_26_duplicate_projectile_change_is_atomic() {
    let mut save = projectile_save();
    save.change_forms
        .push(moved_projectile(0x400A02, 42, [31.0, 32.0, 33.0]));
    let mut world = World::new(references::fixture_order());
    let before = world.save_state();

    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.save_state(), before);
}
