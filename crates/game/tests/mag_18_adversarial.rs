// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#[path = "common/mag_adversarial.rs"] mod support;
use support::*;
use soe_esm::Record;
use soe_game::inventory::StackSource;

#[test]
fn scroll_rejects_a_stale_one_item_selection_without_consuming_another_stack() {
    let mut mgef = effect(0, -1);
    put(&mut mgef.subrecords[0].data, 80, 1);
    let mut data = vec![0; 36];
    put(&mut data, 16, 3);
    put(&mut data, 20, 3);
    let scroll = Record::new(T(b"SCRL"), SPELL)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), [10.0f32.to_le_bytes().as_slice(), &0u32.to_le_bytes(), &0u32.to_le_bytes()].concat());
    let mut game = game(mgef, vec![scroll], &[]);
    game.world_mut().add_item(id(ACTOR), id(SPELL), 1);
    let selected = game.world_mut().select_item(id(ACTOR), id(SPELL), StackSource::Plain, 1).unwrap();
    game.world_mut().remove_item(id(ACTOR), id(SPELL), 1, None);
    let error = game.shared_world().borrow_mut().cast_scroll(id(ACTOR), &selected, id(SOURCE), Some(id(SOURCE)), Some(id(ACTOR)), &mut |_, _| unreachable!()).unwrap_err();
    assert!(matches!(error, soe_game::world::magic::MagicError::Inventory(_)));
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(SPELL)), 0);
    assert_eq!(game.world().active_effects().count(), 0);
}
