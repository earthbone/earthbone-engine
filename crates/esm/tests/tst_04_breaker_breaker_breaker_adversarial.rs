#![allow(clippy::expect_used)]

use soe_esm::{FileHeader, Plugin};

#[test]
fn tst_04_parser_accepts_an_empty_plugin_and_repeated_parse() {
    let bytes = Plugin::new(FileHeader::default()).to_bytes();

    for _ in 0..2 {
        let parsed = Plugin::parse(&bytes).expect("empty plugin parses");
        assert!(parsed.records.is_empty());
    }
}
