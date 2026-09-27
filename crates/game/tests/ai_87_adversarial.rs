// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::package::{self, Package};

#[test]
fn ai_87_success_flag_uses_only_bit_zero_for_each_procedure_branch() {
    let template = package::finish(package::close_header(
        package::build_template(0x887, "FlagBoundaries"),
        None,
    ));
    let first = package::with_procedure(template, "Travel", &[], false)
        .sub(Tag(*b"FNAM"), 0x8000_0000u32.to_le_bytes());
    let second = package::with_procedure(first, "Sandbox", &[], false)
        .sub(Tag(*b"FNAM"), 0x0000_0001u32.to_le_bytes());
    let parsed = Package::from_record(&second).unwrap();
    assert!(!parsed.tree[0].success_completes_package);
    assert!(parsed.tree[1].success_completes_package);
}

#[test]
fn ai_87_prcb_repeat_is_independent_of_success_completion() {
    let base = package::finish(package::close_header(
        package::build_template(0x888, "RepeatAndComplete"),
        None,
    ));
    let procedure = package::with_procedure(base, "Travel", &[], true)
        .sub(Tag(*b"PRCB"), [0, 0, 0, 0, 1, 0, 0, 0]);
    let parsed = Package::from_record(&procedure).unwrap();
    assert!(parsed.tree[0].success_completes_package);
    assert!(parsed.tree[0].repeat_when_complete);
}

#[test]
fn ai_87_installed_skyrim_authors_both_completion_policies() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let bytes = std::fs::read(format!("{data}/Skyrim.esm")).unwrap();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", Plugin::parse(&bytes).unwrap()).unwrap();
    let mut completing = false;
    let mut non_completing = false;
    let ids: Vec<GlobalFormId> = order.winners_of(Tag(*b"PACK")).map(|(id, _)| id).collect();
    for id in ids {
        let record = order.winner(id).unwrap();
        let parsed = Package::from_record(record).unwrap_or_else(|error| panic!("PACK {id:?}: {error}"));
        for (index, branch) in parsed.tree.iter().enumerate() {
            if branch.kind != "Simultaneous" { continue; }
            let end = (index + 1 + branch.child_count as usize).min(parsed.tree.len());
            for child in &parsed.tree[index + 1..end] {
                if child.procedure.is_none() { continue; }
                completing |= child.success_completes_package;
                non_completing |= !child.success_completes_package;
            }
        }
    }
    assert!(completing, "Skyrim.esm has a Simultaneous procedure with FNAM bit 0 set");
    assert!(non_completing, "Skyrim.esm has a Simultaneous procedure with FNAM bit 0 clear");
}
