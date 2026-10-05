// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_trace::{Entity, EntityStats, Profile};
use std::collections::BTreeMap;

#[test]
fn prf_13_equal_time_rankings_have_stable_identity_order() {
    let mut totals = BTreeMap::new();
    for id in ["actor-b", "actor-a"] {
        totals.insert(
            Entity::new("actor", id, "Same.esp", "movement"),
            EntityStats {
                calls: 1,
                self_ns: 25,
                ..EntityStats::default()
            },
        );
    }

    let ranking = Profile::ranking(&totals, "actor");
    assert_eq!(
        ranking.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        ["actor-a", "actor-b"]
    );
}
