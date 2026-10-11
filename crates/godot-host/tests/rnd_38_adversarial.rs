// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed hearth material contracts through the production surface builder.

use soe_esm::{records::refr::Refr, Tag};
use soe_godot_host::{data_source::DataSource, nif_effect};
use std::path::Path;

#[test]
fn rnd_38_bannered_mare_palette_flames_are_additive_and_keep_falloff() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open_with_threads(Path::new(&data), 2).unwrap();
    let loaded = source.stream_cell_id(source.resolve_cell("WhiterunBanneredMare").unwrap()).unwrap();
    let order = source.shared_order();
    let mut flames = 0;
    for surface in &loaded.surfaces {
        let Some(owner) = order.winner_ref(surface.reference) else { continue };
        let Ok(refr) = Refr::from_record(order.record(owner)) else { continue };
        let Some(base) = order.winner(order.resolve(owner.file, refr.base)) else { continue };
        let model = base.find(Tag::new(b"MODL")).map(soe_esm::cursor::zstring).unwrap_or_default();
        if !model.to_ascii_lowercase().contains("fireplacewood01burning") { continue; }
        let Some(effect) = &surface.arrays.material.effect else { continue };
        let flags = surface.arrays.material.shader_flags_1;
        assert_ne!(flags & soe_nif::blocks::flags1::GREYSCALE_TO_PALETTE_COLOR, 0);
        assert!(surface.textures.diffuse.is_some(), "flame texture resolves");
        assert!(surface.textures.greyscale.is_some(), "effect palette resolves");
        let shader = nif_effect::shader(&surface.arrays.material, effect);
        assert!(shader.contains("blend_add"), "flame is additive");
        assert!(shader.contains("ALPHA = opacity;"), "alpha follows flame texture");
        assert!(shader.contains("palette_texture, vec2(texel.g, tint.r)"), "hue comes from palette");
        assert!(shader.contains("angle_opacity = mix("), "authored falloff remains active");
        flames += 1;
    }
    assert!(flames > 0, "Bannered Mare fireplace flame has a palette surface");
}
