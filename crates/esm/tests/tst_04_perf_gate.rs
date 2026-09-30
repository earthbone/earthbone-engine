#![allow(clippy::expect_used)]

use soe_esm::{FileHeader, Plugin, Record};

#[test]
fn tst_04_esm_parser_reads_the_full_synthetic_plugin() {
    let mut source = Plugin::new(FileHeader::default());
    for id in 1..=128 {
        source.push_top(Record::new(*b"MISC", id).zsub(*b"EDID", &format!("Tst04_{id}")));
    }

    let parsed = Plugin::parse(&source.to_bytes()).expect("synthetic plugin parses");
    assert_eq!(parsed.records.len(), 128);
    assert_eq!(
        parsed.records[127].editor_id().as_deref(),
        Some("Tst04_128")
    );
}
