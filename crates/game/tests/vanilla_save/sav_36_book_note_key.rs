// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const BOOK: u32 = 0x810;
const NOTE: u32 = 0x811;
const KEY: u32 = 0x812;

fn book_note_key_order() -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, light) in [
        ("Skyrim.esm", false),
        ("Extra.esl", true),
        ("B.esp", false),
        ("A.esp", false),
        ("Light.esl", true),
    ] {
        let mut plugin = plugin(light);
        if name == "Skyrim.esm" {
            plugin.push_top(
                Record::new(Tag(*b"BOOK"), BOOK)
                    .sub(Tag(*b"EDID"), b"TestBook\0".to_vec())
                    .sub(Tag(*b"FULL"), b"Authored Book\0".to_vec()),
            );
            plugin.push_top(
                Record::new(Tag(*b"NOTE"), NOTE)
                    .sub(Tag(*b"EDID"), b"TestNote\0".to_vec())
                    .sub(Tag(*b"FULL"), b"Authored Note\0".to_vec()),
            );
            plugin.push_top(
                Record::new(Tag(*b"KEYM"), KEY)
                    .sub(Tag(*b"EDID"), b"TestKey\0".to_vec())
                    .sub(Tag(*b"FULL"), b"Authored Key\0".to_vec()),
            );
        }
        order.add(name, plugin).unwrap();
    }
    order
}

fn saved_change(form: u32, type_index: u8, flags: u32, payload: Vec<u8>) -> soe_save::ChangeForm {
    soe_save::ChangeForm {
        form: reference(form),
        type_field: type_index,
        version: 0,
        change_flags: flags,
        data: payload,
        uncompressed_len: 0,
    }
}

fn changed_save() -> Save {
    let mut saved = save();
    saved.change_forms = vec![
        saved_change(
            0x400000 | BOOK,
            13,
            1 | 2 | 4 | (1 << 5) | (1 << 6),
            [
                0x20_u32.to_le_bytes().as_slice(),
                7_u16.to_le_bytes().as_slice(),
                &[8],
            ]
            .concat(),
        ),
        saved_change(
            0x400000 | NOTE,
            28,
            1 | (1 << 31),
            [
                0x40_u32.to_le_bytes().as_slice(),
                9_u16.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
        saved_change(
            0x400000 | KEY,
            25,
            1 | 2 | 4,
            [
                0x80_u32.to_le_bytes().as_slice(),
                11_u16.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
    ];
    saved
}

#[test]
fn sav_36_book_note_key_changes_are_atomic_and_visible_after_engine_save_load() {
    let order = book_note_key_order();
    let saved = changed_save();
    let mut world = World::new(order);
    assert_eq!(world.restore_vanilla_book_note_keys(&saved).unwrap(), 3);

    for (form, flags, name) in [
        (BOOK, 0x20, "Authored Book"),
        (NOTE, 0x40, "Authored Note"),
        (KEY, 0x80, "Authored Key"),
    ] {
        let id = id(0, form);
        assert_eq!(world.record(id).unwrap().flags, flags);
        assert_eq!(world.name_of(id).as_deref(), Some(name));
    }
    let book = world.restored_book_note_key_extra(id(0, BOOK)).unwrap();
    assert!(book.value_changed && book.full_name_changed && book.teaches_skill && book.read);
    assert_eq!(book.opaque_suffix, [8]);
    assert!(
        world
            .restored_book_note_key_extra(id(0, NOTE))
            .unwrap()
            .read
    );
    let key = world.restored_book_note_key_extra(id(0, KEY)).unwrap();
    assert!(key.value_changed && key.full_name_changed);

    let mut loaded = World::new(book_note_key_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.save_state(), world.save_state());
    for form in [BOOK, NOTE, KEY] {
        assert_eq!(loaded.record(id(0, form)), world.record(id(0, form)));
        assert_eq!(loaded.name_of(id(0, form)), world.name_of(id(0, form)));
        assert_eq!(
            loaded.restored_book_note_key_extra(id(0, form)),
            world.restored_book_note_key_extra(id(0, form))
        );
    }
}

#[test]
fn sav_36_late_invalid_change_does_not_install_earlier_record_overrides() {
    let mut saved = changed_save();
    saved.change_forms[2].change_flags |= 1 << 7;
    let mut world = World::new(book_note_key_order());
    let before = world.save_state();
    assert!(world.restore_vanilla_book_note_keys(&saved).is_err());
    assert_eq!(world.save_state(), before);
}
