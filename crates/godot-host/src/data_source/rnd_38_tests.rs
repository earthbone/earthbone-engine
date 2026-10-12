// SPDX-License-Identifier: GPL-3.0-or-later

//! Hearth distortion is a lighting material, separate from palette flames.

use super::{apply_refraction_material, fixture, material_desc_from};
use crate::mesh::MaterialKey;
use soe_nif::blocks::{flags1, Block};

fn fixture_file() -> soe_nif::NifFile {
    let mut writer = fixture::NifWriter::new(100);
    let texture = writer.texture_set(&["textures/distortion.dds"]);
    let texture = writer.add("BSShaderTextureSet", texture);
    let shader = writer.lighting_shader_emission(
        0,
        flags1::REFRACTION | flags1::ZBUFFER_TEST,
        0,
        texture,
        [0.0; 4],
        &[],
    );
    let shader = writer.add("BSLightingShaderProperty", shader);
    let (vertices, triangles) = fixture::quad();
    let shape = writer.bs_tri_shape(
        "Heat",
        [0.0; 3],
        -1,
        shader,
        -1,
        soe_nif::vertex::attr::VERTEX,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", shape);
    writer.roots = vec![shape];
    let mut file = soe_nif::parse(&writer.finish()).unwrap();
    for block in &mut file.blocks {
        if let Block::LightingShader(shader) = block {
            shader.refraction_strength = 0.03125;
            shader.alpha = 0.375;
            shader.uv_offset = [0.125, 0.25];
            shader.uv_scale = [2.0, 4.0];
            shader.clamp_mode = 2;
        }
    }
    file
}

#[test]
fn rnd_38_distortion_preserves_authored_inputs_and_material_identity() {
    let file = fixture_file();
    let meshes = file.meshes().unwrap();
    let mut material = material_desc_from(&meshes[0]);
    let ordinary_key = MaterialKey::from(&material);
    apply_refraction_material(&file, &meshes[0], &mut material);
    let refraction = material.refraction.as_ref().unwrap();
    assert_eq!(refraction.strength, 0.03125);
    assert_eq!(refraction.alpha, 0.375);
    assert_eq!(refraction.uv_offset, [0.125, 0.25]);
    assert_eq!(refraction.uv_scale, [2.0, 4.0]);
    assert_eq!(refraction.clamp_mode, 2);
    assert!(material.effect.is_none());
    let distortion_key = MaterialKey::from(&material);
    assert_ne!(distortion_key, ordinary_key);
    material.refraction.as_mut().unwrap().strength *= 2.0;
    assert_ne!(MaterialKey::from(&material), distortion_key);
    let mut ordinary = file.clone();
    for block in &mut ordinary.blocks {
        if let Block::LightingShader(shader) = block {
            shader.flags1 &= !flags1::REFRACTION;
        }
    }
    let meshes = ordinary.meshes().unwrap();
    let mut material = material_desc_from(&meshes[0]);
    apply_refraction_material(&ordinary, &meshes[0], &mut material);
    assert!(material.refraction.is_none());
}

#[test]
fn rnd_38_distortion_samples_the_background_instead_of_emitting_normal_rgb() {
    let file = fixture_file();
    let meshes = file.meshes().unwrap();
    let material = material_desc_from(&meshes[0]);
    let shader = crate::nif_refraction::shader(&material);
    assert!(shader.contains("depth_draw_never"));
    assert!(shader.contains("hint_screen_texture"));
    assert!(shader.contains("ALBEDO = textureLod(screen_texture,"));
    assert!(shader.contains("distortion.rg * 2.0 - 1.0"));
    assert!(shader.contains("float opacity = distortion.a * base_alpha * 1.0;"));
    assert!(shader.contains("if (opacity < 0.1) { discard; }"));
    let mut vertex_alpha = material.clone();
    vertex_alpha.shader_flags_1 |= flags1::VERTEX_ALPHA;
    assert!(crate::nif_refraction::shader(&vertex_alpha)
        .contains("distortion.a * base_alpha * COLOR.a"));
    assert!(!shader.contains("ALBEDO = distortion"));
}

#[test]
fn rnd_38_installed_hearth_normal_plane_is_refraction_not_fire_colour() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let archive =
        soe_bsa::Archive::open(&std::path::PathBuf::from(data).join("Skyrim - Meshes0.bsa"))
            .unwrap();
    for name in ["fireplacewood01burning", "campfire01burning"] {
        let bytes = archive
            .read(&format!("meshes/clutter/woodfires/{name}.nif"))
            .unwrap();
        let file = super::placed_model(&bytes).unwrap();
        let meshes = file.meshes().unwrap();
        let mut distortion_planes = 0;
        for mesh in meshes {
            let mut material = material_desc_from(&mesh);
            apply_refraction_material(&file, &mesh, &mut material);
            if material
                .diffuse
                .as_deref()
                .is_some_and(|path| path.to_ascii_lowercase().contains("vaportilenormal"))
            {
                distortion_planes += 1;
                assert_ne!(material.shader_flags_1 & flags1::REFRACTION, 0);
                assert!(material.effect.is_none());
                assert!(material.refraction.is_some());
                println!("RND-38 {name}: {} {:#?}", mesh.name, material.refraction);
            }
        }
        assert_eq!(distortion_planes, 1, "{name} heat plane");
    }
}
