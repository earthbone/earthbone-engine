// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, Plugin, Record};

#[test]
fn parser_rejects_every_incomplete_prefix_of_the_benchmark_plugin() {
    let mut source = Plugin::new(FileHeader::default());
    for id in 1..=128 {
        source.push_top(Record::new(*b"MISC", id).zsub(*b"EDID", &format!("Tst04_{id}")));
    }
    let bytes = source.to_bytes();
    let header_only_len = Plugin::new(FileHeader::default()).to_bytes().len();

    for end in 0..bytes.len() {
        if end == header_only_len {
            let parsed = Plugin::parse(&bytes[..end]).expect("header-only plugin is valid");
            assert!(parsed.records.is_empty());
        } else {
            assert!(
                Plugin::parse(&bytes[..end]).is_err(),
                "parser accepted truncated plugin prefix of {end} bytes"
            );
        }
    }
}
