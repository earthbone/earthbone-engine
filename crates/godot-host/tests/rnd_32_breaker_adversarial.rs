// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent installed-data count check for enabled placed models.

use std::collections::HashSet;
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
        let parent = order.resolve(owner.file, raw);
        return enabled(order, parent, seen) ^ (xesp[4] & 1 != 0);
    }
    matches!(record.tag.as_bytes(), b"REFR" | b"ACHR")
        && record.flags & flags::INITIALLY_DISABLED == 0
}

fn has_model(base: &soe_esm::Record) -> bool {
    let has_path = if base.tag == Tag::new(b"ARMO") {
        [b"MOD2", b"MOD4"]
            .into_iter()
            .find(|field| {
                base.find(Tag::new(field))
                    .is_some_and(|bytes| !soe_esm::cursor::zstring(bytes).is_empty())
            })
            .is_some()
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

#[test]
fn rnd_32_installed_build_counts_equal_eligible_winning_references() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA unset, skipping RND-32 installed count check");
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 4).unwrap();
    let order = source.shared_order();
    let mut manifest = Vec::new();

    for name in CELLS {
        let cell = source.resolve_cell(name).unwrap();
        let loaded = source.stream_cell_id(cell).unwrap();
        let built: HashSet<_> = loaded
            .surfaces
            .iter()
            .filter(|surface| {
                order
                    .winner(surface.reference)
                    .is_some_and(|r| r.tag == Tag::new(b"REFR"))
            })
            .map(|surface| surface.reference)
            .collect();
        let mut eligible = HashSet::new();
        let mut plants = Vec::new();
        for &id in source.cell_index().references(cell) {
            let Some(owner) = order.winner_ref(id) else {
                continue;
            };
            let reference = order.record(owner);
            if reference.tag != Tag::new(b"REFR") || reference.is_deleted() {
                continue;
            }
            let Some(raw) = reference
                .find(Tag::new(b"NAME"))
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
            else {
                continue;
            };
            let Some(base) = order.winner(order.resolve(owner.file, u32::from_le_bytes(raw)))
            else {
                continue;
            };
            let marker = matches!(base.tag.as_bytes(), b"STAT" | b"ACTI" | b"FURN" | b"DOOR")
                && base.flags & (1 << 23) != 0;
            let editor = [b"XOCP", b"XORD", b"XRMR", b"XPOD", b"XPTL"]
                .iter()
                .any(|field| reference.find(Tag::new(field)).is_some());
            let marker_only = loaded.stats.reference_skips.get(&id) == Some(&"marker model");
            if enabled(&order, id, &mut HashSet::new())
                && has_model(base)
                && !marker
                && !editor
                && !marker_only
            {
                eligible.insert(id);
                if matches!(base.tag.as_bytes(), b"TREE" | b"FLOR") {
                    let placement = Refr::from_record(reference).unwrap();
                    plants.push(serde_json::json!({
                        "file": id.file,
                        "object": id.object,
                        "kind": base.tag.to_string(),
                        "position": placement.transform.unwrap().position,
                    }));
                }
            }
        }
        assert_eq!(
            built, eligible,
            "{name}: drawn ids must match enabled model references"
        );
        let counts = &loaded.stats.reference_counts;
        assert_eq!(
            counts.values().map(|c| c.built).sum::<usize>(),
            built.len(),
            "{name}: built count must be unique references"
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
                "{name} {kind}: outcomes must exhaust placed refs"
            );
        }
        manifest.push(serde_json::json!({
            "cell": name,
            "id": cell.to_string(),
            "counts": counts.iter().map(|(kind, count)| (kind.to_string(), serde_json::json!({"built": count.built}))).collect::<serde_json::Map<_, _>>(),
            "objects": plants,
        }));
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::write(
        target.join("rnd_32_whiterun.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
