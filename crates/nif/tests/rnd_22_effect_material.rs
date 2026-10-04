// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Effect materials preserve the nifly color, scale, opacity and alpha contract.

mod common;
use common::{quad, NifWriter};
use soe_nif::{
    blocks::{flags1, flags2, BlendFactor},
    mesh::MaterialKind,
    vertex::attr,
};

#[test]
fn rnd_22_effect_fields_and_alpha_factors_survive_mesh_extraction() {
    let mut writer = NifWriter::new(100);
    let shader = writer.effect_shader(
        flags1::USE_FALLOFF | flags1::SOFT_EFFECT,
        flags2::DOUBLE_SIDED,
        "textures/effect.dds",
        [0.2, 0.3, 0.4, 0.5],
    );
    let shader = writer.add("BSEffectShaderProperty", shader);
    let alpha = writer.alpha_property(1 | (6 << 1) | (7 << 5), 0);
    let alpha = writer.add("NiAlphaProperty", alpha);
    let (vertices, triangles) = quad();
    let shape = writer.bs_tri_shape(
        "Effect",
        [0.0; 3],
        -1,
        shader,
        alpha,
        attr::VERTEX | attr::UVS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    writer.roots = vec![shape];
    let file = soe_nif::parse(&writer.finish()).unwrap();
    let meshes = file.meshes().unwrap();
    let m = &meshes[0].material;
    assert_eq!(m.kind, MaterialKind::Effect);
    assert_eq!(m.emissive_color, [0.2, 0.3, 0.4]);
    assert_eq!(m.emissive_multiple, 2.0);
    assert_eq!(m.alpha, 0.5);
    let effect = m.effect.as_ref().unwrap();
    assert_eq!(effect.clamp_mode, 3);
    assert_eq!(
        [
            effect.falloff_start_angle,
            effect.falloff_stop_angle,
            effect.falloff_start_opacity,
            effect.falloff_stop_opacity
        ],
        [1.0, 1.0, 0.75, 0.25]
    );
    assert_eq!(effect.soft_falloff_depth, 100.0);
    let blend = m.blend.unwrap();
    assert_eq!(
        (blend.src, blend.dst),
        (BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha)
    );
}

#[test]
fn rnd_22_installed_effect_assets_keep_authored_shading() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let mut count = 0;
    for name in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"] {
        let archive = soe_bsa::Archive::open(&directory.join(name)).unwrap();
        for path in archive.files().iter().filter(|p| {
            std::path::Path::new(p)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("nif"))
                && (p.contains("mist")
                    || p.contains("windowglow")
                    || p.contains("lightbeam")
                    || p.contains("wrtercloud"))
        }) {
            let bytes = archive.read(path).unwrap();
            let file = soe_nif::parse(&bytes).unwrap();
            for mesh in file.meshes().unwrap() {
                let material = mesh.material;
                if let Some(effect) = material.effect {
                    assert_eq!(
                        material.emissive_color,
                        [
                            effect.base_color[0],
                            effect.base_color[1],
                            effect.base_color[2]
                        ],
                        "{path}"
                    );
                    assert_eq!(material.alpha, effect.base_color[3], "{path}");
                    if path.contains("wrtercloud") || path.contains("whfxwindowglow01.nif") {
                        eprintln!("{path}: flags={:x}/{:x} blend={:?} color={:?} scale={} falloff={}/{}/{}/{} soft={} palette={}", material.flags1, material.flags2, material.blend, effect.base_color, effect.base_color_scale, effect.falloff_start_angle, effect.falloff_stop_angle, effect.falloff_start_opacity, effect.falloff_stop_opacity, effect.soft_falloff_depth, effect.greyscale_texture);
                    }
                    count += 1;
                }
            }
        }
    }
    assert!(
        count >= 20,
        "installed mist and window effect survey: {count}"
    );
}
