// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::plugin::flags::{DELETED, INITIALLY_DISABLED, PERSISTENT};

#[test]
fn sav_54_mask_only_changes_the_authored_enable_and_persistent_bits() {
    let unrelated = 0x8000_0200;
    for authored in [
        0,
        INITIALLY_DISABLED,
        PERSISTENT,
        INITIALLY_DISABLED | PERSISTENT,
    ] {
        for (mask, set_word) in [
            (unrelated | INITIALLY_DISABLED, 0),
            (unrelated | PERSISTENT, u16::MAX),
            (unrelated | INITIALLY_DISABLED | PERSISTENT, 0xA55A),
        ] {
            let mut save = super::fixture();
            super::set_form_mask(&mut save.change_forms[1], mask, set_word, 78);
            let mut world =
                super::World::new(super::fixture_order_with_flags(authored | unrelated));
            world.restore_vanilla_reference_placements(&save).unwrap();

            let reference = world.reference_ref(super::id(0, 0xA00)).unwrap();
            let effective = authored ^ mask;
            assert_eq!(reference.enabled, effective & INITIALLY_DISABLED == 0);
            assert_eq!(reference.persistent, effective & PERSISTENT != 0);
        }
    }
}

#[test]
fn sav_54_mask_cannot_clear_the_deleted_winner_state() {
    let mut save = super::fixture();
    super::set_form_mask(
        &mut save.change_forms[1],
        INITIALLY_DISABLED | PERSISTENT | DELETED,
        u16::MAX,
        78,
    );
    let mut world = super::World::new(super::fixture_order_with_flags(DELETED));
    world.restore_vanilla_reference_placements(&save).unwrap();

    let reference = world.reference_ref(super::id(0, 0xA00)).unwrap();
    assert!(!reference.enabled);
}
