// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! XPRM decoding and exact byte preservation for placed reference primitives.

use std::path::PathBuf;

use soe_esm::records::refr::{Primitive, Refr};
use soe_esm::{LoadOrder, Plugin, Tag};

fn xprm() -> [u8; 32] {
    let bits = [
        0x7fc0_1234_u32,
        0x8000_0000,
        0x3f80_0000,
        0x0000_0001,
        0x3f00_0000,
        0x7fa0_0001,
        0x3e80_0000,
        0xbf00_0000,
    ];
    let mut bytes = [0; 32];
    for (index, value) in bits.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[28..32].copy_from_slice(&3_u32.to_le_bytes());
    bytes
}

#[test]
fn fmt_16_xprm_decodes_bounds_color_and_kind_and_round_trips_float_bits() {
    let bytes = xprm();
    let primitive = Primitive::decode(&bytes).unwrap();
    assert_eq!(primitive.bounds[0].to_bits(), 0x7fc0_1234);
    assert_eq!(primitive.bounds[1].to_bits(), 0x8000_0000);
    assert_eq!(primitive.bounds[2].to_bits(), 0x3f80_0000);
    assert_eq!(primitive.color[0].to_bits(), 0x0000_0001);
    assert_eq!(primitive.color[1].to_bits(), 0x3f00_0000);
    assert_eq!(primitive.color[2].to_bits(), 0x7fa0_0001);
    assert_eq!(primitive.color[3].to_bits(), 0x3e80_0000);
    assert_eq!(primitive.kind, 3);
    assert_eq!(primitive.encode(), bytes);

    let record = soe_esm::Record::new(b"REFR", 1)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"XPRM", bytes);
    assert_eq!(
        Refr::from_record(&record)
            .unwrap()
            .primitive
            .unwrap()
            .encode(),
        bytes
    );
}

#[test]
fn fmt_16_xprm_requires_one_fixed_size_field() {
    for length in [0, 1, 31, 33] {
        assert!(
            Primitive::decode(&vec![0; length]).is_err(),
            "length {length}"
        );
    }

    let repeated = soe_esm::Record::new(b"REFR", 1)
        .sub(b"NAME", 2_u32.to_le_bytes())
        .sub(b"XPRM", [0; 32])
        .sub(b"XPRM", [0; 32]);
    assert!(Refr::from_record(&repeated).is_err());
}

#[test]
fn fmt_16_installed_reference_xprm_survey_decodes_and_round_trips() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping XPRM survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }

    let mut with_primitive = 0usize;
    let mut kinds = [0usize; 5];
    let mut errors = Vec::new();
    for (id, record) in order.winners_of(Tag::new(b"REFR")) {
        if record.is_deleted() {
            continue;
        }
        let Some(original) = record.find(Tag::new(b"XPRM")) else {
            continue;
        };
        with_primitive += 1;
        match Primitive::decode(original) {
            Ok(primitive) => {
                if primitive.encode().as_slice() != original {
                    errors.push(format!("REFR {id} XPRM did not round-trip exactly"));
                }
                if let Some(count) = kinds.get_mut(primitive.kind as usize) {
                    *count += 1;
                }
                match Refr::from_record(record) {
                    Ok(refr)
                        if refr.primitive.map(|view| view.encode()) == Some(primitive.encode()) => {
                    }
                    Ok(_) => errors.push(format!("REFR {id} view differs from its XPRM")),
                    Err(error) => errors.push(format!("REFR {id}: {error}")),
                }
            }
            Err(error) => errors.push(format!("REFR {id}: {error}")),
        }
    }
    assert!(
        with_primitive > 0,
        "no winning references with XPRM were surveyed"
    );
    assert!(
        kinds.iter().sum::<usize>() > 0,
        "the installed XPRM survey did not find a documented kind value"
    );
    assert!(
        errors.is_empty(),
        "{} XPRM survey failures:\n{}",
        errors.len(),
        errors.join("\n")
    );
    eprintln!("{with_primitive} XPRM references surveyed, kinds {kinds:?}");
}
