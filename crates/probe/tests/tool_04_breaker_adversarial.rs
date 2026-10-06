// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! TOOL-04 adversarial cases for source identity and paired observations.

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_probe::{compare, run, Probe, EXAMPLE};

fn synthetic_order(probe: &Probe) -> LoadOrder {
    let mut order = LoadOrder::new();
    for name in probe.load_order.iter().take(probe.load_order.len() - 1) {
        let mut plugin = Plugin::new(FileHeader::default());
        for form in probe.forms.values().filter(|form| form.plugin == *name) {
            plugin.push_top(Record::new(Tag(*b"MISC"), form.object));
        }
        if name == "Skyrim.esm" {
            plugin.push_top(
                Record::new(Tag(*b"GLOB"), 0x3a)
                    .zsub(Tag(*b"EDID"), "TimeScale")
                    .sub(Tag(*b"FNAM"), *b"f")
                    .sub(Tag(*b"FLTV"), probe.timescale.to_le_bytes()),
            );
        }
        order.add(name, plugin).unwrap();
    }
    order
}

#[test]
fn matching_traces_with_a_false_authored_load_order_do_not_match() {
    let probe = Probe::parse(EXAMPLE).unwrap();
    let mut engine = run(&probe, synthetic_order(&probe)).unwrap();
    let mut captured = engine.clone();

    for observation in engine.iter_mut().chain(captured.iter_mut()) {
        observation.load_order = vec!["Skyrim.esm".into()];
    }

    let report = compare(&probe, &engine, &captured).unwrap();
    assert!(
        !report.matched,
        "a pair of matching observations must still match the authored load order"
    );
}
