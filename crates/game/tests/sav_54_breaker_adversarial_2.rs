// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::vanilla_save::{
    id,
    references::{fixture, fixture_order_with_flags, set_form_mask},
};
use soe_game::World;

use soe_esm::plugin::flags::{INITIALLY_DISABLED, PERSISTENT};
use soe_save::Save;

fn masked_save(mask: u32, set_word: u16, save_version: u32, form_version: u8) -> Save {
    let mut save = fixture();
    save.header.version = save_version;
    save.form_version = form_version;
    set_form_mask(&mut save.change_forms[1], mask, set_word, form_version);
    save
}

#[test]
fn sav_54_mask_invert_toggles_authored_enable_and_persistence_across_versions() {
    for (save_version, form_version) in [(12, 77), (12, 78), (13, 77), (13, 78)] {
        for authored in [
            0,
            INITIALLY_DISABLED,
            PERSISTENT,
            INITIALLY_DISABLED | PERSISTENT,
        ] {
            for mask in [
                INITIALLY_DISABLED,
                PERSISTENT,
                INITIALLY_DISABLED | PERSISTENT,
            ] {
                let save = masked_save(mask, 0, save_version, form_version);
                let mut world = World::new(fixture_order_with_flags(authored));
                world.restore_vanilla_reference_placements(&save).unwrap();

                let effective = authored ^ mask;
                let reference = world.reference_ref(id(0, 0xA00)).unwrap();
                assert_eq!(
                    reference.enabled,
                    effective & INITIALLY_DISABLED == 0,
                    "save={save_version} form={form_version} authored={authored:#x} mask={mask:#x}"
                );
                assert_eq!(
                    reference.persistent,
                    effective & PERSISTENT != 0,
                    "save={save_version} form={form_version} authored={authored:#x} mask={mask:#x}"
                );
            }
        }
    }
}

#[test]
fn sav_54_inverted_reference_persistence_survives_engine_save_load() {
    let authored = INITIALLY_DISABLED;
    let save = masked_save(INITIALLY_DISABLED | PERSISTENT, 0x0050, 12, 78);
    let mut world = World::new(fixture_order_with_flags(authored));
    world.restore_vanilla_reference_placements(&save).unwrap();

    let state = world.save_state();
    let mut loaded = World::new(fixture_order_with_flags(authored));
    loaded.load_state(&state).unwrap();
    let reference = loaded.reference_ref(id(0, 0xA00)).unwrap();
    assert!(reference.enabled);
    assert!(reference.persistent);
    assert!(loaded.is_persistent_child(id(0, 0x800), id(0, 0xA00)));
}

#[test]
fn sav_54_masked_restore_rejects_repeat_without_changing_state() {
    let save = masked_save(INITIALLY_DISABLED, 0, 12, 78);
    let mut world = World::new(fixture_order_with_flags(0));
    world.restore_vanilla_reference_placements(&save).unwrap();
    let before = world.state_hash();

    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.state_hash(), before);
}
