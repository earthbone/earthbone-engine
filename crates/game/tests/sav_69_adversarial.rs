// SPDX-License-Identifier: GPL-3.0-or-later

//! Adversarial ownership and lock restoration checks from SAV-69.

use super::*;
use soe_save::ChangeForm;

fn extra_change(target: u32, flags: u32, entries: &[u8]) -> ChangeForm {
    ChangeForm {
        form: reference(target),
        change_flags: flags,
        type_field: 0,
        version: 0,
        data: entries.to_vec(),
        uncompressed_len: 0,
    }
}

fn owned_locked_entries(owner: u32, rank: i32, key: u32, lock_flags: u8) -> Vec<u8> {
    let mut data = vec![12, 33];
    reference(owner).write(&mut data);
    data.push(35);
    data.extend(rank.to_le_bytes());
    data.push(42);
    data.extend([77, lock_flags]);
    reference(key).write(&mut data);
    data.extend(0xAABB_CCDD_u32.to_le_bytes());
    data.extend(0x1234_5678_u32.to_le_bytes());
    data
}

#[test]
fn sav_69_signed_rank_null_lock_key_and_unknown_words_survive_engine_save() {
    let mut save = references::fixture();
    save.change_forms = vec![extra_change(
        0x400A00,
        1 << 31,
        &owned_locked_entries(0x400007, i32::MIN, 0, 5),
    )];
    let mut world = World::new(references::fixture_order());
    world.reference(id(0, 0xA00)).unwrap();

    let report = world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!((report.references, report.owners, report.locks), (1, 1, 1));
    assert_eq!(report.leveled_locks.into_iter().collect::<Vec<_>>(), [id(0, 0xA00)]);
    let state = world.reference_ref(id(0, 0xA00)).unwrap();
    assert_eq!(state.owner, Some(id(0, 7)));
    assert_eq!(state.owner_rank, Some(i32::MIN));
    assert!(state.locked);
    assert_eq!(world.key(id(0, 0xA00)), None);
    let lock = state.lock_extra.unwrap();
    assert_eq!(lock.flags, 4);
    assert_eq!(lock.unknown, [0xAABB_CCDD, 0x1234_5678]);
    assert!(world.pending.is_empty());

    let bytes = world.save_state();
    let mut loaded = World::new(references::fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.reference_ref(id(0, 0xA00)), Some(state));
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(loaded.save_state(), bytes);
}

#[test]
fn sav_69_empty_flagged_extra_list_clears_owner_and_lock_without_events() {
    let mut save = references::fixture();
    save.change_forms = vec![extra_change(
        0x400A00,
        (1 << 31) | (1 << 6) | (1 << 12),
        &owned_locked_entries(0x400007, -9, 0, 1),
    )];
    let mut world = World::new(references::fixture_order());
    world.reference(id(0, 0xA00)).unwrap();
    world.restore_vanilla_reference_extras(&save).unwrap();

    save.change_forms[0].data = vec![0];
    let report = world.restore_vanilla_reference_extras(&save).unwrap();
    assert_eq!((report.references, report.owners, report.locks), (1, 1, 1));
    let state = world.reference_ref(id(0, 0xA00)).unwrap();
    assert_eq!(state.owner, None);
    assert_eq!(state.owner_rank, None);
    assert_eq!(state.owner_global, None);
    assert!(!state.locked);
    assert_eq!(state.lock_level, 0);
    assert_eq!(world.key(id(0, 0xA00)), None);
    assert!(world.pending.is_empty());
}

#[test]
fn sav_69_late_invalid_owner_link_leaves_prior_target_state_unchanged() {
    let mut save = references::fixture();
    let good = extra_change(
        0x400A00,
        1 << 31,
        &owned_locked_entries(0x400007, 17, 0, 1),
    );
    let mut bad_data = vec![4, 33];
    reference(0x400900).write(&mut bad_data);
    let bad = extra_change(0x400A00, 1 << 31, &bad_data);
    save.change_forms = vec![good, bad];
    let mut world = World::new(references::fixture_order());
    world.reference(id(0, 0xA00)).unwrap();
    let before = world.save_state();
    let hash = world.state_hash();

    let error = world.restore_vanilla_reference_extras(&save).unwrap_err();
    assert!(error.to_string().contains("invalid ownership or lock link"));
    assert_eq!(world.save_state(), before);
    assert_eq!(world.state_hash(), hash);
    assert!(world.pending.is_empty());
}
