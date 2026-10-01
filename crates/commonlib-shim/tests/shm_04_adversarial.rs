// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

#[test]
fn stale_actor_handles_can_be_reported_as_empty_actor_pointers() {
    let header = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/RE/Reference.h"),
    )
    .unwrap();
    assert!(header.contains("struct ActorPtr { std::optional<ActorHandle> handle; }"));
    assert!(header.contains("virtual ActorPtr actor_ptr() const = 0;"));
    assert!(
        !header.contains("return {actor_handle()};"),
        "default actor_ptr always wraps actor_handle and cannot report an expired handle"
    );
}
