// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The authored identity context is the comparison contract, not just a label.

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
fn tool_04_adversarial_rejects_matching_traces_with_wrong_authored_identity() {
    let probe = Probe::parse(EXAMPLE).unwrap();
    let mut engine = run(&probe, synthetic_order(&probe)).unwrap();
    let mut observed = engine.clone();

    // The protocol defines these as the declared enabled load order and the
    // measured Form.GetFormID values for the authored aliases.
    for trace in [&mut engine, &mut observed] {
        for observation in trace {
            observation.load_order = vec!["Skyrim.esm".into()];
            observation.forms.insert("runner".into(), 0x1234);
        }
    }

    let report = compare(&probe, &engine, &observed).unwrap();
    assert!(
        !report.matched,
        "equal traces must still fail when their context violates the authored probe"
    );
}
