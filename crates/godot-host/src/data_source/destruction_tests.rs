// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed DEST model survey for the CMB-37 RenderingServer replacement path.

use std::path::PathBuf;

use super::*;

#[test]
fn cmb_37_vanilla_destruction_models_build_host_surfaces() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed CMB-37 model survey");
        return;
    };
    let source = DataSource::open_with_threads(&data, 1).expect("open installed data");
    let mut candidates = Vec::new();
    for (reference, record) in source.order.winners_of(soe_esm::tag::REFR) {
        let Ok(refr) = Refr::from_record(record) else {
            continue;
        };
        let Some(winner) = source.order.winner_ref(reference) else {
            continue;
        };
        let base = source.order.resolve(winner.file, refr.base);
        let Some(base) = source.order.winner(base) else {
            continue;
        };
        if base.find(soe_esm::Tag::new(b"DEST")).is_none()
            || base.all(soe_esm::Tag::new(b"DSTD")).next().is_none()
        {
            continue;
        }
        for bytes in base.all(soe_esm::Tag::new(b"DMDL")) {
            let model = bytes
                .split(|byte| *byte == 0)
                .next()
                .filter(|path| !path.is_empty())
                .map(String::from_utf8_lossy)
                .map(|path| path.into_owned());
            if let Some(model) = model {
                candidates.push((reference, model));
            }
        }
    }
    candidates.sort();
    candidates.dedup();

    let mut failures = Vec::new();
    for (reference, model) in candidates {
        match source.stream_destruction_model(reference, &model) {
            Ok(loaded) if !loaded.surfaces.is_empty() => {
                assert!(loaded.stats.triangles > 0);
                return;
            }
            Ok(_) => failures.push(format!("{reference} {model}: no drawable shapes")),
            Err(error) => failures.push(format!("{reference} {model}: {error}")),
        }
    }
    panic!("no installed DEST DMDL built drawable surfaces: {failures:?}");
}
