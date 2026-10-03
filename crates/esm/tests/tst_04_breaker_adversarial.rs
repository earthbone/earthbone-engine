#![allow(clippy::expect_used, clippy::panic)]

use soe_esm::Plugin;

#[test]
fn parser_rejects_a_truncated_plugin_header() {
    let mut bytes = soe_esm::Plugin::new(soe_esm::FileHeader::default()).to_bytes();
    bytes.truncate(8);

    assert!(Plugin::parse(&bytes).is_err());
}
