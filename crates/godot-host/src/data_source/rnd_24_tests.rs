// SPDX-License-Identifier: GPL-3.0-or-later

//! Emission flag and glow-map coverage in installed models, RND-24.

use super::*;

#[test]
fn rnd_24_installed_chandelier_uses_flagged_glow_materials() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping RND-24 installed mesh check");
        return;
    };
    let source = DataSource::open_with_threads(std::path::Path::new(&data), 1).unwrap();
    let mut found = Vec::new();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    for reference in source.cell_index().references(cell) {
        let Some(owner) = source.order.winner_ref(*reference) else {
            continue;
        };
        let record = source.order.record(owner);
        if record.tag != soe_esm::tag::REFR {
            continue;
        }
        let Ok(refr) = Refr::from_record(record) else {
            continue;
        };
        let base = source.order.resolve(owner.file, refr.base);
        let Some(base_record) = source.order.winner(base) else {
            continue;
        };
        let Some(model) = model_for(base_record) else {
            continue;
        };
        if !model.to_ascii_lowercase().contains("chand") {
            continue;
        }
        let bytes = source.vfs.read(&format!("meshes/{model}"));
        let Ok(bytes) = bytes else {
            continue;
        };
        let Ok(file) = placed_model(&bytes) else {
            continue;
        };
        let meshes = file.meshes().unwrap();
        let emissive: Vec<_> = meshes
            .iter()
            .filter(|mesh| {
                let material = &mesh.material;
                (material.flags1 & soe_nif::blocks::flags1::OWN_EMIT != 0
                    || material.flags2 & soe_nif::blocks::flags2::GLOW_MAP != 0)
                    && material.emissive_multiple > 0.0
                    && material.emissive_color.iter().any(|value| *value > 0.0)
            })
            .collect();
        let masked: Vec<_> = emissive
            .iter()
            .filter(|mesh| {
                (mesh.material.flags2 & soe_nif::blocks::flags2::GLOW_MAP != 0
                    && mesh.material.glow().is_some())
                    || (mesh.material.flags1 & soe_nif::blocks::flags1::OWN_EMIT != 0
                        && mesh
                            .material
                            .glow()
                            .is_some_and(|path| path.to_ascii_lowercase().ends_with("_g.dds")))
            })
            .collect();
        found.push((model, emissive.len(), masked.len(), meshes.len()));
    }
    found.sort();
    found.dedup();
    eprintln!("RND-24 Bannered Mare chandeliers: {found:?}");
    assert!(
        found
            .iter()
            .any(|(_, emissive, masked, _)| *emissive > 0 && *masked > 0),
        "installed Bannered Mare has no placed chandelier with flagged glow-map emission"
    );
}
