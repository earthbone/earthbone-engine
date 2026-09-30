// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-66 regression coverage for the opaque spatial change-form boundary.

use soe_save::{ChangeForm, RefId};

#[test]
fn sav_66_raw_spatial_payloads_preserve_observed_flag_boundaries() {
    let cases = [
        (6, 0x6000_0000, 8),
        (38, 0x4000_0000, 29),
        (38, 0x8000_0000, 2),
        (29, 0x8000_0000, 16),
    ];

    for (kind, flags, len) in cases {
        let data = (0..len).map(|byte| byte as u8).collect::<Vec<_>>();
        let form = ChangeForm {
            form: RefId::from_bytes([0x40, 1, 2]),
            change_flags: flags,
            type_field: kind,
            version: 78,
            uncompressed_len: 0,
            data: data.clone(),
        };

        assert_eq!(form.payload().unwrap().as_ref(), data);

        let mut encoded = Vec::new();
        form.write(&mut encoded);
        assert_eq!(u32::from_le_bytes(encoded[3..7].try_into().unwrap()), flags);
        assert_eq!(encoded[7] & 0x3f, kind);
        assert_eq!(&encoded[11..], data);
    }
}
