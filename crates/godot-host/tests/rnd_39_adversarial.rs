// SPDX-License-Identifier: GPL-3.0-or-later

//! Require the installed waterfall survey to find additive foam or mist.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::data_source::DataSource;
use std::path::Path;

#[test]
fn rnd_39_whiterun_waterfalls_have_additive_foam_and_mist_surfaces() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut additive = 0;
    let mut scrolling = 0;
    let loaded = source.stream_exterior("WhiterunOrigin", 2).unwrap();
    let order = source.shared_order();
    let mut waterfall_models = std::collections::BTreeSet::new();
    for (_, cell) in &loaded.cells {
        for surface in &cell.surfaces {
            let Some(owner) = order.winner_ref(surface.reference) else { continue };
            let record = order.record(owner);
            if record.tag != soe_esm::tag::REFR { continue; }
            let Ok(refr) = Refr::from_record(record) else { continue };
            let base = order.resolve(owner.file, refr.base);
            let Some(model) = order
                .winner(base)
                .and_then(|record| record.find(Tag::new(b"MODL")))
                .map(soe_esm::cursor::zstring)
            else {
                continue;
            };
            let lower = model.to_ascii_lowercase();
            if !lower.contains("waterfall") && !lower.contains("fxwaterfall") {
                continue;
            }
            waterfall_models.insert(model.clone());
            let Some(effect) = surface.arrays.material.effect.as_ref() else { continue };
            if effect
                .blend
                .is_some_and(|blend| blend.dst == soe_nif::blocks::BlendFactor::One)
            {
                assert!(surface.textures.diffuse.is_some(), "foam texture resolves for {model}");
                assert!(soe_godot_host::nif_effect::shader(&surface.arrays.material, effect)
                    .contains("blend_add"));
                additive += 1;
            }
            if effect.controllers.iter().any(|track| matches!(track.channel, 6 | 8)) {
                assert!(effect.blend.is_some(), "scrolling falls preserve alpha blend");
                scrolling += 1;
            }
        }
    }
    assert!(!waterfall_models.is_empty(), "Whiterun waterfall placements found");
    assert!(additive > 0, "waterfall foam or mist renders additively");
    assert!(scrolling > 0, "waterfall surface scrolls with authored alpha blend");
}
