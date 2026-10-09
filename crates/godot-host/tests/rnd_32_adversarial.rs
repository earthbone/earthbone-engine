// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed placed-reference accounting against an independent REFR oracle.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use soe_esm::{plugin::flags, records::refr::Refr, GlobalFormId, Tag};
use soe_godot_host::data_source::DataSource;

const CELLS: [&str; 4] = [
    "WhiterunBanneredMare",
    "WhiterunExterior04",
    "WhiterunExterior13",
    "WhiterunExterior17",
];

fn enabled(order: &soe_esm::LoadOrder, id: GlobalFormId, seen: &mut HashSet<GlobalFormId>) -> bool {
    if !seen.insert(id) {
        return false;
    }
    let Some(owner) = order.winner_ref(id) else {
        return false;
    };
    let record = order.record(owner);
    if record.is_deleted() {
        return false;
    }
    if let Some(xesp) = record.find(Tag::new(b"XESP")) {
        if xesp.len() != 8 {
            return false;
        }
        let raw = u32::from_le_bytes(xesp[..4].try_into().unwrap());
        if raw == 0 {
            return false;
        }
        return enabled(order, order.resolve(owner.file, raw), seen) ^ (xesp[4] & 1 != 0);
    }
    record.flags & flags::INITIALLY_DISABLED == 0
}

fn model_bearing(base: &soe_esm::Record) -> bool {
    let has_path = if base.tag == Tag::new(b"ARMO") {
        [b"MOD2", b"MOD4"].into_iter().any(|field| {
            base.find(Tag::new(field))
                .is_some_and(|bytes| !soe_esm::cursor::zstring(bytes).is_empty())
        })
    } else {
        base.find(Tag::new(b"MODL"))
            .is_some_and(|bytes| !soe_esm::cursor::zstring(bytes).is_empty())
    };
    has_path
        && matches!(
            base.tag.as_bytes(),
            b"STAT"
                | b"ACTI"
                | b"FURN"
                | b"DOOR"
                | b"CONT"
                | b"GRAS"
                | b"TREE"
                | b"FLOR"
                | b"LIGH"
                | b"MSTT"
                | b"SCOL"
                | b"TACT"
                | b"MISC"
                | b"WEAP"
                | b"ARMO"
                | b"BOOK"
                | b"INGR"
                | b"ALCH"
                | b"KEYM"
                | b"SLGM"
                | b"AMMO"
                | b"SCRL"
                | b"APPA"
        )
}

fn editor_reference(record: &soe_esm::Record) -> bool {
    [b"XOCP", b"XORD", b"XRMR", b"XPOD", b"XPTL"]
        .iter()
        .any(|field| record.find(Tag::new(field)).is_some())
}

#[test]
fn rnd_32_installed_cells_match_winning_enable_and_model_eligibility() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping RND-32 installed adversarial check");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 4).unwrap();
    let order = source.shared_order();
    let mut manifest = Vec::new();

    for name in CELLS {
        let cell = source.resolve_cell(name).unwrap();
        let loaded = source.stream_cell_id(cell).unwrap();
        let drawn: BTreeSet<_> = loaded
            .surfaces
            .iter()
            .filter(|surface| {
                order
                    .winner(surface.reference)
                    .is_some_and(|record| record.tag == Tag::new(b"REFR"))
            })
            .map(|surface| surface.reference)
            .collect();
        let mut expected = BTreeSet::new();
        let mut plants = Vec::new();

        for &id in source.cell_index().references(cell) {
            let Some(owner) = order.winner_ref(id) else {
                continue;
            };
            let reference = order.record(owner);
            if reference.tag != Tag::new(b"REFR") || reference.is_deleted() {
                continue;
            }
            assert_eq!(
                soe_game::World::authored_reference_enabled(&order, id),
                enabled(&order, id, &mut HashSet::new()),
                "{name} {id} winning XESP or Initially Disabled result"
            );
            let Some(raw_base) = reference
                .find(Tag::new(b"NAME"))
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .map(u32::from_le_bytes)
            else {
                continue;
            };
            let Some(base) = order.winner(order.resolve(owner.file, raw_base)) else {
                continue;
            };
            let is_base_marker =
                matches!(base.tag.as_bytes(), b"STAT" | b"ACTI" | b"FURN" | b"DOOR")
                    && base.flags & (1 << 23) != 0;
            let is_marker_only = loaded.stats.reference_skips.get(&id) == Some(&"marker model");
            if enabled(&order, id, &mut HashSet::new())
                && model_bearing(base)
                && !is_base_marker
                && !editor_reference(reference)
                && !is_marker_only
            {
                expected.insert(id);
                if matches!(base.tag.as_bytes(), b"TREE" | b"FLOR") {
                    let placement = Refr::from_record(reference).unwrap();
                    let position = placement.transform.unwrap().position;
                    plants.push(serde_json::json!({
                        "file": id.file,
                        "object": id.object,
                        "kind": base.tag.to_string(),
                        "position": position,
                    }));
                }
            }
        }

        assert_eq!(
            drawn, expected,
            "{name} built references differ from source eligibility"
        );
        let counts = &loaded.stats.reference_counts;
        let built: usize = counts.values().map(|count| count.built).sum();
        assert_eq!(
            built,
            drawn.len(),
            "{name} built count must count unique REFRs"
        );
        for (kind, count) in counts {
            assert_eq!(
                count.placed,
                count.built
                    + count.initially_disabled
                    + count.enable_parent
                    + count.runtime_disabled
                    + count.markers
                    + count.other.values().sum::<usize>(),
                "{name} {kind} outcomes must be disjoint and exhaustive"
            );
        }
        let serialized_counts: serde_json::Map<String, serde_json::Value> = counts
            .iter()
            .map(|(kind, count)| (kind.to_string(), serde_json::json!({"built": count.built})))
            .collect();
        manifest.push(serde_json::json!({
            "cell": name,
            "id": cell.to_string(),
            "counts": serialized_counts,
            "objects": plants,
        }));
    }

    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("rnd_32_whiterun.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
