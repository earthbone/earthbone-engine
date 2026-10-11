#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;

use soe_esm::{FileHeader, Plugin, Record, Tag};

// This inventory is an independent oracle for every ordinary typed link
// field currently exposed by the ESM record views. Specialized layouts such
// as CTDA, VMAD, and embedded gameplay fields have their own FMT-15 tests.
const SCALAR_LINKS: &[(&[u8; 4], &[u8; 4])] = &[
    (b"CELL", b"LTMP"),
    (b"CELL", b"XCWT"),
    (b"CELL", b"XOWN"),
    (b"CELL", b"XLCN"),
    (b"CELL", b"XCIM"),
    (b"CELL", b"XCMO"),
    (b"CELL", b"XCAS"),
    (b"CELL", b"XEZN"),
    (b"CELL", b"XCCM"),
    (b"WRLD", b"WNAM"),
    (b"WRLD", b"CNAM"),
    (b"WRLD", b"NAM2"),
    (b"WRLD", b"NAM3"),
    (b"WRLD", b"XLCN"),
    (b"WRLD", b"XEZN"),
    (b"WRLD", b"ZNAM"),
    (b"LCTN", b"PNAM"),
    (b"LCTN", b"MNAM"),
    (b"NPC_", b"PNAM"),
    (b"EQUP", b"PNAM"),
    (b"REFR", b"NAME"),
    (b"REFR", b"XOWN"),
    (b"REFR", b"XLCN"),
    (b"REFR", b"XLYR"),
    (b"REFR", b"XEZN"),
    (b"REFR", b"XEMI"),
    (b"REFR", b"XTNM"),
    (b"REFR", b"XMBR"),
    (b"REFR", b"XLRT"),
    (b"REFR", b"XLRL"),
    (b"REFR", b"INAM"),
    (b"LTEX", b"TNAM"),
    (b"WEAP", b"EITM"),
    (b"WEAP", b"BIDS"),
    (b"WEAP", b"BAMT"),
    (b"WEAP", b"EFSD"),
    (b"WEAP", b"INAM"),
    (b"WEAP", b"WNAM"),
    (b"WEAP", b"SNAM"),
    (b"WEAP", b"XNAM"),
    (b"WEAP", b"NAM7"),
    (b"WEAP", b"TNAM"),
    (b"WEAP", b"UNAM"),
    (b"WEAP", b"NAM9"),
    (b"WEAP", b"NAM8"),
    (b"WEAP", b"CNAM"),
    (b"WEAP", b"ETYP"),
    (b"WEAP", b"YNAM"),
    (b"WEAP", b"ZNAM"),
    (b"ARMO", b"EITM"),
    (b"ARMO", b"RNAM"),
    (b"ARMO", b"TNAM"),
    (b"NAVM", b"ONAM"),
    (b"SCEN", b"PNAM"),
    (b"DIAL", b"QNAM"),
    (b"INFO", b"TPIC"),
    (b"INFO", b"PNAM"),
    (b"INFO", b"DNAM"),
    (b"INFO", b"ANAM"),
    (b"INFO", b"TWAT"),
    (b"INFO", b"ONAM"),
    (b"INFO", b"SNAM"),
    (b"INFO", b"LNAM"),
    (b"MGEF", b"KWDA"),
    (b"MGEF", b"ESCE"),
    (b"HAZD", b"MNAM"),
    (b"INFO", b"TCLT"),
    (b"COBJ", b"CNAM"),
    (b"COBJ", b"BNAM"),
];

fn ids(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|id| id.to_le_bytes()).collect()
}

#[test]
fn fmt_15_independent_oracle_preserves_every_mapped_typed_reference_after_reload() {
    let mapping = HashMap::from([(0x800, 0x900), (0x801, 0x901), (0x802, 0x902)]);
    let mut plugin = Plugin::new(FileHeader::default());
    let mut expected = Vec::new();

    for &(record, field) in SCALAR_LINKS {
        let values = if field == b"KWDA" {
            vec![0x801, 0x802]
        } else {
            vec![0x801]
        };
        let data = ids(&values);
        plugin.push_top(Record::new(record, 0x800).sub(field, data));
        expected.push((
            Tag::new(record),
            Tag::new(field),
            ids(&[0x901, 0x902][..values.len()]),
        ));
    }

    for (record, field) in [(b"LTEX", b"GNAM"), (b"CELL", b"XCLR")] {
        let data = ids(&[0x801, 0x802]);
        plugin.push_top(Record::new(record, 0x800).sub(field, data));
        expected.push((Tag::new(record), Tag::new(field), ids(&[0x901, 0x902])));
    }

    plugin.remap_form_ids(&mapping).unwrap();
    let reloaded = Plugin::parse(&plugin.to_bytes()).unwrap();
    assert_eq!(reloaded.records.len(), expected.len());
    for (record, field, expected_data) in expected {
        let found = reloaded
            .records
            .iter()
            .find(|candidate| candidate.tag == record && candidate.find(field).is_some())
            .unwrap_or_else(|| panic!("missing {record:?} {field:?}"));
        assert_eq!(found.form_id, 0x900, "record identity {record:?}");
        assert_eq!(
            found.find(field).unwrap(),
            expected_data,
            "{record:?} {field:?}"
        );
    }
}
