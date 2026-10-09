// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-29 diagnostic history remains bounded during repeated loads.

use soe_trace::{LoadCause, LoadScope, LoadTrace};

#[test]
fn prf_29_history_evicts_oldest_and_reports_eviction() {
    let first = LoadScope::new("first", LoadCause::Parse, "old.nif");
    let first_id = first.id();
    drop(first);
    for _ in 0..65_536 {
        let _load = LoadScope::new("retention", LoadCause::Decode, "texture.dds");
    }
    let events = LoadTrace::events();
    assert_eq!(events.len(), 65_536);
    assert!(!events.iter().any(|event| event.id == first_id));
    assert!(LoadTrace::summary_text().contains("evicted=1"));
}
