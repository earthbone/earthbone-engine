#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

fn mapping() -> HashMap<u32, u32> {
    HashMap::from([
        (0x800, 0x900),
        (0x801, 0x901),
        (0x802, 0x902),
        (0x803, 0x903),
        (0x804, 0x904),
    ])
}

#[test]
fn fmt_15_cobj_typed_links_are_renumbered() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(b"COBJ", 0x800)
            .sub(b"CNAM", 0x801u32.to_le_bytes())
            .sub(b"BNAM", 0x802u32.to_le_bytes())
            .sub(b"CNTO", [0x803u32.to_le_bytes(), 1i32.to_le_bytes()].concat())
            .sub(
                b"COED",
                [0x804u32.to_le_bytes(), 77u32.to_le_bytes(), 1.0f32.to_le_bytes()].concat(),
            ),
    );

    plugin.remap_form_ids(&mapping()).unwrap();
    let record = &plugin.records[0];

    assert_eq!(record.form_id, 0x900);
    assert_eq!(record.find(Tag::new(b"CNAM")).unwrap(), &0x901u32.to_le_bytes());
    assert_eq!(record.find(Tag::new(b"BNAM")).unwrap(), &0x902u32.to_le_bytes());
    assert_eq!(
        &record.find(Tag::new(b"CNTO")).unwrap()[..4],
        &0x903u32.to_le_bytes()
    );
    assert_eq!(
        &record.find(Tag::new(b"COED")).unwrap()[..4],
        &0x904u32.to_le_bytes()
    );
    assert_eq!(
        &record.find(Tag::new(b"COED")).unwrap()[4..8],
        &77u32.to_le_bytes()
    );
}

fn ctda_record() -> Record {
    let mut condition = [0u8; 32];
    condition[0] = 4; // Comparison union selects a Global FormID.
    condition[4..8].copy_from_slice(&0x801u32.to_le_bytes());
    condition[20..24].copy_from_slice(&2u32.to_le_bytes()); // Run On: Reference.
    condition[24..28].copy_from_slice(&0x802u32.to_le_bytes());
    Record::new(b"QUST", 0x800).sub(b"CTDA", condition)
}

#[test]
fn fmt_15_ctda_global_comparison_link_is_renumbered() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(ctda_record());
    plugin.remap_form_ids(&mapping()).unwrap();
    let data = plugin.records[0].find(Tag::new(b"CTDA")).unwrap();

    assert_eq!(&data[4..8], &0x901u32.to_le_bytes());
}

#[test]
fn fmt_15_ctda_run_on_reference_link_is_renumbered() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(ctda_record());
    plugin.remap_form_ids(&mapping()).unwrap();
    let data = plugin.records[0].find(Tag::new(b"CTDA")).unwrap();

    assert_eq!(&data[24..28], &0x902u32.to_le_bytes());
}
