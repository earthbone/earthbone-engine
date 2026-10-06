// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_probe::{ScheduleRow, ScheduleTable};

#[test]
fn ply_07_comparison_reports_a_named_actor_identity_mismatch() {
    let rows = |name: &str| {
        (0..24)
            .map(|hour| ScheduleRow {
                hour,
                actor: 0x0001_3b93,
                name: name.into(),
                package: Some(0x1000),
                cell: Some(0x2000),
                furniture: None,
            })
            .collect()
    };
    let engine = ScheduleTable::new(rows("Eorlund Gray-Mane")).unwrap();
    let observed = ScheduleTable::new(rows("Different actor name")).unwrap();

    let differences = engine.differences(&observed);

    assert_eq!(differences.len(), 24);
    assert!(differences.iter().all(|difference| {
        difference.hour < 24
            && difference.actor == 0x0001_3b93
            && difference.field == "name"
    }));
}
