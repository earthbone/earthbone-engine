// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for RND-11's grass placement inputs: the `GRAS` and
//! `LTEX` record readers `godot-host/src/data_source/grass.rs` depends on.
//! These attack the parsers directly, at the crate boundary they actually
//! promise: `Grass::from_record` and `LandTexture::from_record` reject or
//! correctly decode a malformed or boundary `DATA`/`OBND`/`TNAM`/`GNAM`
//! block, using only the public fixture builder (`Record::new().sub()`),
//! no `SKYRIM_DATA` needed.
//!
//! Run:
//! ```text
//! cargo test -p soe-esm --test gras_ltex_adversarial
//! ```

use soe_esm::records::gras::{Grass, WaterDistanceType};
use soe_esm::records::ltex::LandTexture;
use soe_esm::{tag, Error, Record};

/// Builds a well-formed 32 byte `GRAS` `DATA` block, the layout
/// `Grass::from_record` reads in `crates/esm/src/records/gras.rs`:
/// density, min_slope, max_slope, 1 pad byte, units_from_water (u16),
/// 2 pad bytes, units_from_water_type (u32), position_range, height_range,
/// color_range, wave_period (f32 each), flags, 3 pad bytes. Cross checked
/// against `TES5Edit/Core/wbDefinitionsTES5.pas`'s `wbRecord(GRAS, ...)`.
fn data_block(units_from_water_type_raw: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(32);
    b.push(10); // density
    b.push(0); // min_slope
    b.push(90); // max_slope
    b.push(0); // pad
    b.extend_from_slice(&0u16.to_le_bytes()); // units_from_water
    b.extend_from_slice(&[0u8; 2]); // pad
    b.extend_from_slice(&units_from_water_type_raw.to_le_bytes());
    b.extend_from_slice(&0.0f32.to_le_bytes()); // position_range
    b.extend_from_slice(&0.0f32.to_le_bytes()); // height_range
    b.extend_from_slice(&0.0f32.to_le_bytes()); // color_range
    b.extend_from_slice(&0.0f32.to_le_bytes()); // wave_period
    b.push(0); // flags
    b.extend_from_slice(&[0u8; 3]); // pad
    assert_eq!(
        b.len(),
        32,
        "test fixture itself must build a 32 byte block"
    );
    b
}

/// `TES5Edit/Core/wbDefinitionsTES5.pas`'s `wbRecord(GRAS, ...)` defines
/// `Units From Water Type` as an 8 value enum: 0 Above-AtLeast,
/// 1 Above-AtMost, 2 Below-AtLeast, 3 Below-AtMost, 4 Either-AtLeast,
/// 5 Either-AtMost, 6 Either-AtMostAbove, 7 Either-AtMostBelow.
/// `Mutagen/Mutagen.Bethesda.Skyrim/Records/Major Records/Grass.cs`
/// declares the same 8 names in the same order, independently confirming
/// it. Raw value 1 is an "above water" variant in both sources: it must
/// not decode as `WaterDistanceType::BelowWater`.
#[test]
fn units_from_water_type_above_at_most_is_not_decoded_as_below_water() {
    let record = Record::new(tag::GRAS, 1).sub(b"DATA", data_block(1));
    let grass = Grass::from_record(&record).expect("well formed 32 byte DATA block must parse");
    assert_ne!(
        grass.units_from_water_type,
        WaterDistanceType::BelowWater,
        "raw units_from_water_type 1 is Above-AtMost per TES5Edit's wbDefinitionsTES5.pas \
         and Mutagen's Grass.cs, an above water variant, not below water"
    );
}

/// Same source as above. Raw value 2 is a "below water" variant
/// (Below-AtLeast) in both TES5Edit and Mutagen, not the absent third
/// state the Rust enum calls `Unused`, which appears nowhere in either
/// corpus's 8 value list.
#[test]
fn units_from_water_type_below_at_least_is_not_decoded_as_unused() {
    let record = Record::new(tag::GRAS, 1).sub(b"DATA", data_block(2));
    let grass = Grass::from_record(&record).expect("well formed 32 byte DATA block must parse");
    assert_eq!(
        grass.units_from_water_type,
        WaterDistanceType::BelowWater,
        "raw units_from_water_type 2 is Below-AtLeast per TES5Edit's wbDefinitionsTES5.pas \
         and Mutagen's Grass.cs, a below water variant, not the Unused state the current \
         decode maps it to"
    );
}

/// `GRAS`'s `DATA` is a fixed 32 byte block in every corpus source this
/// crate's own doc comment cites (TES5Edit, Mutagen, OpenMW's
/// `loadgras.cpp`). A shorter block is not a partially readable record,
/// it is a plugin the reader must not silently under-parse.
#[test]
fn data_block_shorter_than_32_bytes_is_rejected() {
    let short = data_block(0);
    let record = Record::new(tag::GRAS, 1).sub(b"DATA", &short[..20]);
    let err = Grass::from_record(&record).expect_err("a 20 byte DATA block must be rejected");
    assert!(
        matches!(err, Error::SubrecordSize { .. }),
        "expected a SubrecordSize error for a short DATA block, got {err:?}"
    );
}

/// The same 32 byte contract the other direction: a longer block is not a
/// forward compatible extension the reader should tolerate by reading the
/// first 32 bytes and ignoring the rest, it is a plugin whose layout this
/// reader does not recognise.
#[test]
fn data_block_longer_than_32_bytes_is_rejected() {
    let mut long = data_block(0);
    long.extend_from_slice(&[0u8; 8]);
    let record = Record::new(tag::GRAS, 1).sub(b"DATA", long);
    let err = Grass::from_record(&record).expect_err("a 40 byte DATA block must be rejected");
    assert!(
        matches!(err, Error::SubrecordSize { .. }),
        "expected a SubrecordSize error for an oversized DATA block, got {err:?}"
    );
}

/// `OBND` is documented in `gras.rs` as "min x y z, max x y z", six `i16`
/// fields, 12 bytes. An 8 byte block is short by exactly one coordinate
/// pair, silently reading it would either panic or read past the field
/// into whatever comes next.
#[test]
fn obnd_wrong_length_is_rejected() {
    let record = Record::new(tag::GRAS, 1)
        .sub(b"DATA", data_block(0))
        .sub(b"OBND", [0u8; 8]);
    let err = Grass::from_record(&record).expect_err("an 8 byte OBND must be rejected");
    assert!(
        matches!(err, Error::SubrecordSize { .. }),
        "expected a SubrecordSize error for a malformed OBND, got {err:?}"
    );
}

/// `LTEX`'s `TNAM` is documented in `ltex.rs` as a single FormID (4
/// bytes), cross checked against OpenMW's `loadltex.cpp`. A 2 byte field
/// is not a valid FormID and must not be treated as an absent, optional
/// texture set.
#[test]
fn ltex_malformed_tnam_length_errors_instead_of_treating_as_absent() {
    let record = Record::new(tag::LTEX, 1).sub(b"TNAM", [0u8; 2]);
    let err = LandTexture::from_record(&record)
        .expect_err("a 2 byte TNAM must be rejected, not treated as an absent texture set");
    assert!(
        matches!(err, Error::SubrecordSize { .. }),
        "expected a SubrecordSize error for a malformed TNAM, got {err:?}"
    );
}

/// `ltex.rs` documents `GNAM` as "repeated: GRAS FormIDs ... as written in
/// the file", one FormID (4 bytes) per subrecord, matching OpenMW's
/// `loadltex.cpp`. A 6 byte `GNAM` is not a whole number of FormIDs and
/// must not be truncated into one.
#[test]
fn ltex_gnam_subrecord_not_a_multiple_of_four_bytes_is_rejected() {
    let record = Record::new(tag::LTEX, 1).sub(b"GNAM", [0u8; 6]);
    let err = LandTexture::from_record(&record)
        .expect_err("a 6 byte GNAM is not a whole number of FormIDs and must be rejected");
    assert!(
        matches!(err, Error::SubrecordSize { .. }),
        "expected a SubrecordSize error for a malformed GNAM, got {err:?}"
    );
}

/// `ltex.rs` documents `grasses` as "as written in the file, in record
/// order" for a texture that names more than one `GRAS` through repeated
/// `GNAM` subrecords, the case `grass_surfaces_for_cell` walks with
/// `for &grass_raw in &ltex.grasses`. Losing that order, or collapsing
/// repeats, would silently drop or reorder which grasses a texture grows.
#[test]
fn ltex_multiple_gnam_subrecords_preserve_file_order() {
    let record = Record::new(tag::LTEX, 1)
        .sub(b"GNAM", 0x0011_1111u32.to_le_bytes())
        .sub(b"GNAM", 0x0022_2222u32.to_le_bytes())
        .sub(b"GNAM", 0x0033_3333u32.to_le_bytes());
    let ltex = LandTexture::from_record(&record).expect("three well formed GNAM must parse");
    assert_eq!(
        ltex.grasses,
        vec![0x0011_1111, 0x0022_2222, 0x0033_3333],
        "repeated GNAM FormIDs must stay in file order, grass_surfaces_for_cell \
         walks them in this order"
    );
}

/// `MNAM` is the MATT FormID footsteps resolve on painted terrain.
#[test]
fn ltex_reads_material_and_snow_flag() {
    let record = Record::new(tag::LTEX, 1)
        .sub(b"MNAM", 0x0001_2345u32.to_le_bytes())
        .sub(b"INAM", 1u32.to_le_bytes());
    let ltex = LandTexture::from_record(&record).unwrap();
    assert_eq!(ltex.material, Some(0x0001_2345));
    assert!(ltex.is_snow);
}
