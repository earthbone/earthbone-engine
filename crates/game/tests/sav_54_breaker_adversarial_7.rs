// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::vanilla_save::{
    id,
    references::{fixture, fixture_order, fixture_order_with_flags, set_form_mask},
};
use soe_esm::plugin::flags::{INITIALLY_DISABLED, PERSISTENT};
use soe_game::World;

#[test]
fn sav_54_actor_and_reference_masks_follow_the_same_change_flag_layout() {
    for (save_version, form_version) in [(12, 77), (12, 78), (13, 77), (13, 78)] {
        for (target, form_index, authored_values) in [
            (id(0, 0x14), 0, [0, 0, 0]),
            (
                id(0, 0xA00),
                1,
                [0, INITIALLY_DISABLED | PERSISTENT, PERSISTENT],
            ),
        ] {
            for (authored, mask) in authored_values.into_iter().zip([
                INITIALLY_DISABLED | PERSISTENT,
                INITIALLY_DISABLED | PERSISTENT,
                INITIALLY_DISABLED,
            ]) {
                let mut save = fixture();
                save.header.version = save_version;
                save.form_version = form_version;
                if form_index == 0 {
                    let change = &mut save.change_forms[form_index];
                    change.change_flags |= soe_save::reference::flags::FORM;
                    change.version = form_version;
                    change.data.splice(
                        35..35,
                        mask.to_le_bytes().into_iter().chain(0_u16.to_le_bytes()),
                    );
                } else {
                    set_form_mask(&mut save.change_forms[form_index], mask, 0, form_version);
                }
                let mut world = World::new(fixture_order_with_flags(authored));
                world.restore_vanilla_reference_placements(&save).unwrap();

                let restored = world.reference_ref(target).unwrap();
                assert_eq!(
                    restored.enabled,
                    (authored ^ mask) & INITIALLY_DISABLED == 0,
                    "ESS {save_version}, form {form_version}, target {target:?}"
                );
                assert_eq!(
                    restored.persistent,
                    (authored ^ mask) & PERSISTENT != 0,
                    "ESS {save_version}, form {form_version}, target {target:?}"
                );
            }
        }
    }
}

#[test]
fn sav_54_absent_mask_preserves_authored_reference_state() {
    for authored in [0, INITIALLY_DISABLED, PERSISTENT, INITIALLY_DISABLED | PERSISTENT] {
        let mut world = World::new(fixture_order_with_flags(authored));
        world.restore_vanilla_reference_placements(&fixture()).unwrap();
        let restored = world.reference_ref(id(0, 0xA00)).unwrap();
        assert_eq!(restored.enabled, authored & INITIALLY_DISABLED == 0);
        assert_eq!(restored.persistent, authored & PERSISTENT != 0);
    }
}

#[test]
fn sav_54_invalid_mask_version_does_not_partially_restore_prior_references() {
    let mut save = fixture();
    let form_version = save.form_version;
    set_form_mask(
        &mut save.change_forms[1],
        INITIALLY_DISABLED | PERSISTENT,
        0xFFFF,
        form_version,
    );
    save.change_forms[1].version = 76;

    let mut world = World::new(fixture_order());
    let before = world.state_hash();
    assert!(world
        .restore_vanilla_reference_placements(&save)
        .is_err());
    assert_eq!(world.state_hash(), before);
    assert!(world.reference_ref(id(0, 0x14)).is_none());
    assert!(world.reference_ref(id(0, 0xA00)).is_none());
}
