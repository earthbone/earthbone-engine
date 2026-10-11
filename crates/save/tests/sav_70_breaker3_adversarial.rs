// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_save::{reference::flags, ChangeForm, RefId};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn placement() -> Vec<u8> {
    let mut data = Vec::new();
    reference(0x400800).write(&mut data);
    for value in [1.25_f32, -2.0, 3.5, 0.0, 0.5, 2.0] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data
}

#[test]
fn sav_70_form_flag_alone_is_not_a_typed_pcon_or_parw_payload() {
    let mut pcon_data = placement();
    let placement_len = pcon_data.len();
    pcon_data.extend([0xA1, 0xB2, 0xC3, 0xD4]);
    let pcon = ChangeForm {
        form: reference(0x400A00),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 42,
        version: 0,
        uncompressed_len: 0,
        data: pcon_data,
    };
    let header = pcon.reference_header(78).unwrap().unwrap();
    assert!(!header.carries_reference_data);
    assert_eq!(header.bytes_consumed, placement_len);
    assert_eq!(header.data_bytes().unwrap(), pcon.data[..placement_len]);
    assert_eq!(&pcon.data[header.bytes_consumed..], &[0xA1, 0xB2, 0xC3, 0xD4]);
    assert_eq!(header.bytes_consumed + header.remaining_bytes, pcon.data.len());

    let parw_data = [0xA1, 0xB2, 0xC3, 0xD4];
    let parw = ChangeForm {
        form: reference(0x400A01),
        change_flags: flags::FORM,
        type_field: 47,
        version: 0,
        uncompressed_len: 0,
        data: parw_data.to_vec(),
    };
    assert!(parw.reference_header(78).unwrap().is_none());
    let envelope = parw.parw_change().unwrap();
    assert_eq!(envelope.change_flags, flags::FORM);
    assert_eq!(envelope.data, parw_data);
}
