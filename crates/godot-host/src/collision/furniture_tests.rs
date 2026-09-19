// SPDX-License-Identifier: GPL-3.0-or-later

//! A shipped-cell regression for furniture omitted by the STAT-only path.

use super::*;
use soe_esm::Tag;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn bannered_mare_furniture_is_drawn_and_beds_have_collision() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping furniture integration");
        return;
    };
    let source = DataSource::open(&data).expect("open installed data");
    let cell = source
        .resolve_cell("WhiterunBanneredMare")
        .expect("Bannered Mare exists");
    let loaded = source.stream_cell_id(cell).expect("stream Bannered Mare");
    let (_, collision_stats) = source.cell_colliders(cell).expect("cell collision builds");
    assert_eq!(
        collision_stats.references,
        loaded.stats.statics_with_model + loaded.stats.furniture_with_model,
        "rendering and collision select different model-bearing references"
    );
    let drawn: BTreeSet<_> = loaded.surfaces.iter().map(|s| s.reference).collect();
    let mut furniture = 0;
    let mut beds = 0;
    let mut omitted = Vec::new();
    for &id in source.cell_index().references(cell) {
        let rref = source.order().winner_ref(id).expect("reference resolves");
        let rec = source.order().record(rref);
        if rec.tag != tag::REFR {
            continue;
        }
        let refr = Refr::from_record(rec).expect("reference decodes");
        let base = source
            .order()
            .winner(source.order().resolve(rref.file, refr.base))
            .expect("base resolves");
        if base.tag != Tag::new(b"FURN") || base.flags & (1 << 23) != 0 {
            continue;
        }
        let Some(model) = base.find(Tag::new(b"MODL")).map(soe_esm::cursor::zstring) else {
            continue;
        };
        if model.is_empty() {
            continue;
        }
        furniture += 1;
        if !drawn.contains(&id) {
            omitted.push((id, model.clone()));
        }
        if model.to_ascii_lowercase().contains("bed") {
            beds += 1;
            eprintln!("bed {id}: {model}, placement {:?}", refr.transform);
            let collected = source
                .colliders_of(&model, &refr)
                .expect("bed collision reads");
            assert!(
                collected.colliders.iter().any(Collider::is_static),
                "bed {id} has no supported static collision"
            );
        }
    }
    eprintln!(
        "furniture {furniture}, beds {beds}, omitted {}",
        omitted.len()
    );
    assert!(furniture > 0, "fixture contains no furniture");
    assert!(beds > 0, "fixture contains no beds");
    assert_eq!(loaded.stats.furniture_with_model, furniture);
    assert!(
        omitted.is_empty(),
        "furniture absent from render output: {omitted:?}"
    );
}
