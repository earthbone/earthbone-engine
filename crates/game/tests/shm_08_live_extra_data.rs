// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::extra::{ExtraDataList, ExtraDataType, UnsupportedExtraHook};
use soe_esm::{GlobalFormId, LoadOrder, CREATED_FILE};
use soe_game::inventory::{Extra, Field, Stack};

#[test]
fn shm_08_stack_extras_are_live_through_the_mit_api() {
    let order = LoadOrder::new();
    let enchantment = GlobalFormId {
        file: CREATED_FILE,
        object: 0x123,
    };
    let original = GlobalFormId {
        file: CREATED_FILE,
        object: 0x456,
    };
    let template = GlobalFormId {
        file: CREATED_FILE,
        object: 0x789,
    };
    let stack = Stack::new(
        1,
        vec![
            Extra {
                kind: 0x9B,
                fields: vec![Field::Form(Some(enchantment)), Field::Short(640)],
            },
            Extra {
                kind: 0x49,
                fields: vec![Field::Byte(5)],
            },
            Extra {
                kind: 0x2D,
                fields: vec![Field::Form(Some(original)), Field::Form(Some(template))],
            },
        ],
    )
    .unwrap();
    let backing = stack.commonlib_extra_data(&order);
    let list = ExtraDataList::new(&backing);
    assert!(list.has_type(ExtraDataType::Enchantment));
    assert_eq!(
        list.enchantment().unwrap().enchantment.0,
        order.runtime_id(enchantment)
    );
    assert_eq!(list.enchantment().unwrap().charge, 640);
    assert_eq!(list.enchantment().unwrap().remove_on_unequip, None);
    assert_eq!(list.hotkey().unwrap().slot, Some(5));
    let leveled = list.leveled_creature().unwrap();
    assert_eq!(leveled.original_base.0, order.runtime_id(original));
    assert_eq!(leveled.template_base.0, order.runtime_id(template));
    assert_eq!(
        UnsupportedExtraHook::Add.reason(),
        "ExtraDataList::Add mutates engine-owned list state"
    );
    assert_eq!(
        UnsupportedExtraHook::RemoveByType.reason(),
        "ExtraDataList::RemoveByType mutates engine-owned list state"
    );
}

#[test]
fn shm_08_malformed_or_invalid_extra_values_are_not_exposed() {
    let order = LoadOrder::new();
    let stack = Stack::new(
        1,
        vec![Extra {
            kind: 0x49,
            fields: vec![Field::Byte(8)],
        }],
    )
    .unwrap();
    let backing = stack.commonlib_extra_data(&order);
    let list = ExtraDataList::new(&backing);
    assert!(list.has_type(ExtraDataType::Hotkey));
    assert_eq!(list.hotkey().unwrap().slot, None);
    assert!(list.enchantment().is_none());
    assert!(list.leveled_creature().is_none());
}
