// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_probe::{ScheduleRow, ScheduleTable};

fn day(name: &str) -> ScheduleTable {
    ScheduleTable::new(
        (0..24)
            .map(|hour| ScheduleRow {
                hour,
                actor: 0x0001_3b93,
                name: name.into(),
                package: Some(0x1000),
                cell: Some(0x2000),
                furniture: Some(0x3000),
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn ply_07_empty_schedule_is_not_a_complete_whiterun_day() {
    assert!(ScheduleTable::new(Vec::new()).is_err());
}

#[test]
fn ply_07_comparison_reports_actor_name_disagreement() {
    let engine = day("Eorlund Gray-Mane");
    let observed = day("Eorlund");

    let differences = engine.differences(&observed);

    assert_eq!(differences.len(), 24);
    assert!(differences
        .iter()
        .all(|difference| difference.field == "name"));
}
