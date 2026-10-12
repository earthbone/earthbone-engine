// SPDX-License-Identifier: GPL-3.0-or-later

//! Boundary and duplicate-target checks for saved reference extras.

use super::*;
use soe_save::{reference::Field, ChangeForm};

fn empty_extra_change(target: u32) -> ChangeForm {
    ChangeForm {
        form: reference(target),
        change_flags: 1 << 6,
        type_field: 0,
        version: 0,
        data: vec![0],
        uncompressed_len: 0,
    }
}

#[test]
fn sav_69_top_level_list_stops_before_unparsed_suffix() {
    let mut change = empty_extra_change(0x400A00);
    change.change_flags |= 1 << 31;
    change.data = vec![2, 33, 0, 0, 0, 35];
    change.data.extend((-17_i32).to_le_bytes());
    change.data.push(0xFE);

    let extra = change.reference_extra_data(78).unwrap().unwrap();
    assert_eq!(extra.entries.len(), 2);
    assert!(
        matches!(extra.entries[0].fields.as_slice(), [Field::Reference(id)] if id.value() == 0)
    );
    assert!(
        matches!(extra.entries[1].fields.as_slice(), [Field::Word(rank)] if *rank as i32 == -17)
    );
    assert_eq!(
        extra.data_bytes().unwrap(),
        change.data[..change.data.len() - 1]
    );

    let contents = change.reference_contents(78).unwrap().unwrap();
    assert_eq!(contents.remaining_bytes, 1);
}

#[test]
fn sav_69_duplicate_target_fails_without_applying_first_entry() {
    let mut save = references::fixture();
    let mut world = World::new(references::fixture_order());
    world.restore_vanilla_reference_placements(&save).unwrap();
    let target = id(0, 0xA00);
    let before = world.save_state();
    let hash = world.state_hash();

    save.change_forms = vec![empty_extra_change(0x400A00), empty_extra_change(0x400A00)];
    let error = world.restore_vanilla_reference_extras(&save).unwrap_err();
    assert!(error
        .to_string()
        .contains("duplicate reference extras target"));
    assert_eq!(world.save_state(), before);
    assert_eq!(world.state_hash(), hash);
    assert!(world.pending.is_empty());
    assert!(world.reference_ref(target).is_some());
}

#[test]
fn sav_69_reordered_extras_preserve_unknown_lock_flags_through_save_load() {
    let mut save = references::fixture();
    let mut world = World::new(references::fixture_order());
    world.restore_vanilla_reference_placements(&save).unwrap();
    let mut data = vec![4, 42, 254, 0x81];
    reference(0).write(&mut data);
    data.extend(0xAABB_CCDD_u32.to_le_bytes());
    data.extend(0x1234_5678_u32.to_le_bytes());
    data.push(35);
    data.extend(i32::MIN.to_le_bytes());
    data.push(34);
    reference(0).write(&mut data);
    data.push(33);
    reference(0).write(&mut data);
    save.change_forms = vec![ChangeForm {
        form: reference(0x400A00),
        change_flags: 1 << 31,
        type_field: 0,
        version: 0,
        data,
        uncompressed_len: 0,
    }];

    let report = world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!((report.references, report.owners, report.locks), (1, 1, 1));
    let state = world.reference_ref(id(0, 0xA00)).unwrap();
    assert_eq!(state.owner_rank, Some(i32::MIN));
    assert!(state.locked);
    let lock = state.lock_extra.unwrap();
    assert_eq!(lock.flags, 0x80);
    assert_eq!(lock.unknown, [0xAABB_CCDD, 0x1234_5678]);

    let bytes = world.save_state();
    let mut loaded = World::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.reference_ref(id(0, 0xA00)), Some(state));
    assert_eq!(loaded.save_state(), bytes);
}
