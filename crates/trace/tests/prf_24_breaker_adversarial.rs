// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{metric_snapshot, Metric, METRIC_COUNT};

#[test]
fn prf_24_metric_schema_keeps_unique_names_for_report_slots() {
    assert_eq!(Metric::ALL.len(), METRIC_COUNT);
    let names: Vec<_> = Metric::ALL.iter().map(|metric| metric.name()).collect();
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        METRIC_COUNT
    );
    assert_eq!(metric_snapshot().len(), METRIC_COUNT);
    assert!(Metric::ALL
        .iter()
        .any(|metric| metric.name() == "animation_event_latency_ns"));
}
