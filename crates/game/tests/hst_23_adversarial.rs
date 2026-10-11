// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-23 record-backed actor head selection adversarial cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::appearance::ActorAppearance;
use std::collections::BTreeSet;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn head(object: u32, kind: u32, extra: Option<u32>) -> Record {
    let record = Record::new(Tag(*b"HDPT"), object).sub(Tag(*b"PNAM"), kind.to_le_bytes());
    extra.map_or(record.clone(), |extra| {
        record.sub(Tag(*b"HNAM"), extra.to_le_bytes())
    })
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"RACE"), 1)
            .sub(Tag(*b"MNAM"), [])
            .sub(Tag(*b"HEAD"), 10_u32.to_le_bytes())
            .sub(Tag(*b"FNAM"), [])
            .sub(Tag(*b"HEAD"), 11_u32.to_le_bytes()),
    );
    plugin.push_top(head(10, 1, None));
    plugin.push_top(head(11, 1, None));
    plugin.push_top(head(12, 1, Some(13)));
    plugin.push_top(head(13, 0, Some(12)));
    plugin.push_top(head(14, 2, None));
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 20)
            .sub(Tag(*b"RNAM"), 1_u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), 12_u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), 14_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    order
}

#[test]
fn hst_23_head_selection_uses_npc_parts_before_sex_specific_race_defaults() {
    let actual =
        ActorAppearance::from_order(&order(), Some(id(20)), None, &BTreeSet::new()).unwrap();
    assert_eq!(
        actual
            .head_parts
            .iter()
            .map(|part| part.form)
            .collect::<Vec<_>>(),
        [id(12), id(13), id(14)]
    );
}

#[test]
fn hst_23_cyclic_head_extras_terminate_and_include_each_part_once() {
    let actual =
        ActorAppearance::from_order(&order(), Some(id(20)), None, &BTreeSet::new()).unwrap();
    let unique: BTreeSet<_> = actual.head_parts.iter().map(|part| part.form).collect();
    assert_eq!(unique.len(), actual.head_parts.len());
    assert_eq!(actual.head_parts.len(), 3);
}

#[test]
fn hst_23_missing_npc_head_record_is_rejected_as_invalid_input() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"RACE"), 1).sub(Tag(*b"MNAM"), []));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 20).sub(Tag(*b"PNAM"), 99_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("MissingHead.esm", plugin).unwrap();
    assert!(
        ActorAppearance::from_order(&order, Some(id(20)), Some(id(1)), &BTreeSet::new()).is_err()
    );
}
