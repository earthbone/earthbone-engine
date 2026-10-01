// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::support::magic_adversarial as support;

use soe_esm::Tag;
use support::*;

#[test]
fn mag_59_multiple_rally_entries_apply_in_authored_order_and_expire_cleanly() {
    let rally_a = effect(0x300, 38, 1 << 1, 0);
    let rally_b = effect(0x301, 38, 1 << 1, 24);
    let mut rally_spell = spell(0x310, 0x300, 2, 2.0, 1)
        .sub(Tag(*b"EFID"), 0x301_u32.to_le_bytes())
        .sub(
            Tag(*b"EFIT"),
            [
                5.0_f32.to_le_bytes(),
                0_u32.to_le_bytes(),
                2_u32.to_le_bytes(),
            ]
            .concat(),
        );
    rally_spell.subrecords.shrink_to_fit();
    let mut game = fixture(vec![rally_a, rally_b], vec![rally_spell], false);
    assert_eq!(cast(&mut game, 0x310, TARGET).len(), 2);
    assert_eq!(game.world_mut().actor_value(id(TARGET), "confidence"), 5.0);
    game.tick(2.0);

    assert_eq!(game.world_mut().actor_value(id(TARGET), "confidence"), 1.0);
    assert_eq!(game.world().active_effects().count(), 0);
}
