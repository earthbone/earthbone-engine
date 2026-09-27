// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::gameplay::MagicEffect;
use soe_esm::{Record, Tag};

#[test]
fn fmt_02_adversarial_all_named_offsets_match_mgef_source_fields() {
    let mut data = vec![0; 152];
    for (offset, value) in [
        (28usize, 0x3f00_0001u32), // taper weight
        (72, 0x0100_0801), // projectile
        (76, 0x0100_0802), // explosion
        (112, 0xbf00_0001), // dual cast scale
        (144, 0x447a_0001), // AI score
        (148, 0x3e80_0001), // AI delay
    ] {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let record = Record::new(b"MGEF", 7).sub(b"DATA", data.clone());
    let view = MagicEffect::from_record(&record, false).unwrap();
    assert_eq!(view.data.taper_weight.to_bits(), 0x3f00_0001);
    assert_eq!(view.data.projectile, 0x0100_0801);
    assert_eq!(view.data.explosion, 0x0100_0802);
    assert_eq!(view.data.dual_cast_scale.to_bits(), 0xbf00_0001);
    assert_eq!(view.data.ai_score.to_bits(), 0x447a_0001);
    assert_eq!(view.data.ai_delay_time.to_bits(), 0x3e80_0001);
    assert_eq!(record.find(Tag::new(b"DATA")), Some(data.as_slice()));
}

#[test]
fn fmt_02_adversarial_missing_and_truncated_data_are_errors() {
    assert!(MagicEffect::from_record(&Record::new(b"MGEF", 1), false).is_err());
    for size in [0, 27, 151, 153] {
        let record = Record::new(b"MGEF", 1).sub(b"DATA", vec![0; size]);
        assert!(MagicEffect::from_record(&record, false).is_err(), "size {size}");
    }
}
