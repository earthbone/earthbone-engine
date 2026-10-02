// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, Plugin, Record};

#[test]
fn parser_workload_accepts_empty_plugin_and_empty_record_list() {
    let plugin = Plugin::new(FileHeader::default());
    let parsed = Plugin::parse(&plugin.to_bytes()).unwrap();
    assert!(parsed.records.is_empty());
}

#[test]
fn parser_rejects_truncated_body_instead_of_silently_benchmarking_partial_input() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(*b"MISC", 1).zsub(*b"EDID", "Boundary"));
    let mut bytes = plugin.to_bytes();
    bytes.truncate(bytes.len() - 1);
    assert!(Plugin::parse(&bytes).is_err());
}
