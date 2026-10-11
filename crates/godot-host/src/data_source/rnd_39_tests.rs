// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed waterfall contracts from nifly shader and controller layouts.

use super::DataSource;
use std::path::Path;

#[test]
fn rnd_39_installed_whiterun_waterfall_materials_and_controllers() {
    let Some(directory) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&directory)).unwrap();
    let mut placements = Vec::new();
    for (id, _) in source.cell_index().cells() {
        let Some(grid) = source.cell_index().grid_of(id) else {
            continue;
        };
        if source
            .order
            .winner(grid.world)
            .and_then(|r| r.editor_id())
            .as_deref()
            != Some("WhiterunWorld")
        {
            continue;
        }
        for reference in source.cell_index().references(id) {
            let Some(rref) = source.order.winner_ref(*reference) else {
                continue;
            };
            let record = source.order.record(rref);
            if record.tag != soe_esm::tag::REFR {
                continue;
            }
            let Ok(refr) = soe_esm::records::refr::Refr::from_record(record) else {
                continue;
            };
            let base = source.order.resolve(rref.file, refr.base);
            let Some(model) = source.order.winner(base).and_then(super::model_for) else {
                continue;
            };
            if model.to_ascii_lowercase().contains("water") {
                eprintln!("placement {id} {reference} {model} {:?}", refr.transform);
                let super::LoadedStatic::Surfaces { built, .. } =
                    source.load_static_surfaces(&model, &refr, false).unwrap();
                let bounds = built
                    .iter()
                    .find(|(a, _)| {
                        a.material
                            .effect
                            .as_ref()
                            .is_some_and(|e| !e.controllers.is_empty())
                    })
                    .map(|(a, _)| a.bounds);
                let center = bounds.map(|b| {
                    [
                        b.min.x.midpoint(b.max.x),
                        b.min.y.midpoint(b.max.y),
                        b.min.z.midpoint(b.max.z),
                    ]
                });
                placements.push(serde_json::json!({"cell": id.to_string(), "reference": reference.to_string(), "model": model, "position": refr.transform.unwrap_or_default().position, "center": center}));
            }
        }
    }
    placements.sort_by_key(|row| row["reference"].as_str().unwrap().to_owned());
    let mut scrolling = 0;
    let mut additive = 0;
    let mut lighting = 0;
    for row in &placements {
        let model = row["model"].as_str().unwrap();
        if !model.to_ascii_lowercase().contains("waterfall") {
            continue;
        }
        for (arrays, textures) in source.item_preview_surfaces(model).unwrap() {
            if arrays
                .material
                .texture_transform
                .as_ref()
                .is_some_and(|t| !t.controllers.is_empty())
            {
                assert!(
                    textures.diffuse.is_some(),
                    "{model}: lighting waterfall texture resolves"
                );
                assert!(
                    textures.normal.is_some(),
                    "{model}: moving normal texture resolves"
                );
                lighting += 1;
            }
            let Some(effect) = arrays.material.effect.as_ref() else {
                continue;
            };
            assert!(
                textures.diffuse.is_some(),
                "{model}: authored texture resolves"
            );
            if effect
                .controllers
                .iter()
                .any(|t| matches!(t.channel, 6 | 8))
            {
                assert!(
                    effect.blend.is_some(),
                    "{model}: scrolling fall retains alpha blending"
                );
                scrolling += 1;
            }
            if effect
                .blend
                .is_some_and(|b| b.dst == soe_nif::blocks::BlendFactor::One)
            {
                assert!(crate::nif_effect::shader(&arrays.material, effect).contains("blend_add"));
                additive += 1;
            }
        }
    }
    assert!(
        scrolling > 0,
        "Whiterun falls carry their authored scroll controllers"
    );
    assert!(
        lighting > 0,
        "Whiterun waterfall bodies use lighting shader UV controllers"
    );
    eprintln!("Whiterun scrolling effect surfaces={scrolling}, lighting surfaces={lighting}, authored additive surfaces={additive}");
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../godot/.godot/rnd_39");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(
        output.join("placements.json"),
        serde_json::to_vec_pretty(&placements).unwrap(),
    )
    .unwrap();
}

fn controller_file(flags: u16, frequency: f32) -> (soe_nif::NifFile, usize) {
    use super::fixture::{self, Buf, NifWriter};
    use soe_nif::blocks::{flags1, flags2};
    let mut writer = NifWriter::new(100);
    let shader = writer.reserve("BSEffectShaderProperty");
    let controller = writer.reserve("BSEffectShaderPropertyFloatController");
    let mut data = Buf::default();
    data.u32(3)
        .u32(1)
        .f32(0.0)
        .f32(0.0)
        .f32(1.0)
        .f32(-0.5)
        .f32(2.0)
        .f32(-1.0);
    let keys = writer.add("NiFloatData", data.0);
    let mut data = Buf::default();
    data.f32(0.0).i32(keys);
    let interpolator = writer.add("NiFloatInterpolator", data.0);
    let mut data = Buf::default();
    data.i32(-1)
        .u16(flags)
        .f32(frequency)
        .f32(0.25)
        .f32(0.0)
        .f32(2.0)
        .i32(shader)
        .i32(interpolator)
        .u32(8);
    writer.fill(controller, data.0);
    let mut data = writer.effect_shader(
        flags1::ZBUFFER_TEST | flags1::VERTEX_ALPHA,
        flags2::DOUBLE_SIDED,
        "textures/fall.dds",
        [1.0, 1.0, 1.0, 0.5],
    );
    data[8..12].copy_from_slice(&controller.to_le_bytes());
    writer.fill(shader, data);
    let data = writer.alpha_property(1 | (6 << 1) | (7 << 5), 0);
    let alpha = writer.add("NiAlphaProperty", data);
    let (vertices, triangles) = fixture::quad();
    let data = writer.bs_tri_shape(
        "Fall",
        [0.0; 3],
        -1,
        shader,
        alpha,
        soe_nif::vertex::attr::VERTEX | soe_nif::vertex::attr::UVS | soe_nif::vertex::attr::NORMALS,
        &vertices,
        &triangles,
        true,
    );
    let shape = writer.add("BSTriShape", data);
    writer.roots = vec![shape];
    (soe_nif::parse(&writer.finish()).unwrap(), shape as usize)
}

#[test]
fn rnd_39_authored_uv_rate_phase_and_alpha_survive_material_building() {
    let (file, shape) = controller_file(8, 2.0);
    let tracks = crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(
        (tracks[0].channel, tracks[0].frequency, tracks[0].phase),
        (8, 2.0, 0.25)
    );
    assert_eq!(
        tracks[0]
            .keys
            .keys
            .iter()
            .map(|k| (k.time, k.value))
            .collect::<Vec<_>>(),
        [(0.0, 0.0), (1.0, -0.5), (2.0, -1.0)]
    );
    let meshes = file.meshes().unwrap();
    let mut material = super::material_desc_from(&meshes[0]);
    let effect = material.effect.as_mut().unwrap();
    effect.controllers = tracks;
    assert_eq!(effect.params.base_color[3], 0.5);
    assert_eq!(
        effect.blend.unwrap().dst,
        soe_nif::blocks::BlendFactor::OneMinusSrcAlpha
    );
    let shader = crate::nif_effect::shader(&material, material.effect.as_ref().unwrap());
    assert!(shader.contains("animated_offset.y = effect_track_0"));
    assert!(shader.contains("ALPHA = opacity;"));
    assert!(shader.contains("depth_draw_never"));
}

#[test]
fn rnd_39_different_authored_rates_never_share_material_or_batch() {
    let (file, shape) = controller_file(8, 1.0);
    let mesh = file.meshes().unwrap().remove(0);
    let mut data = super::mesh_data_from(&mesh, &soe_nif::Transform::IDENTITY);
    data.material.effect.as_mut().unwrap().controllers =
        crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    let first = crate::mesh::build_surface(&data, crate::mesh::MeshOptions::default()).unwrap();
    data.material.effect.as_mut().unwrap().controllers[0].frequency = 2.0;
    let second = crate::mesh::build_surface(&data, crate::mesh::MeshOptions::default()).unwrap();
    assert_ne!(
        crate::mesh::MaterialKey::from(&first.material),
        crate::mesh::MaterialKey::from(&second.material)
    );
    assert_ne!(
        crate::batch::BatchKey::of(&first),
        crate::batch::BatchKey::of(&second)
    );
}

#[test]
fn rnd_39_inactive_and_manager_controlled_tracks_stay_unstarted() {
    for flags in [0, 8 | 32] {
        let (file, shape) = controller_file(flags, 1.0);
        assert!(crate::nif_effect_animation::for_shape(&file, shape)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn rnd_39_truncated_nonfinite_and_cyclic_controllers_return_errors() {
    for case in 0..3 {
        let (mut file, shape) = controller_file(8, 1.0);
        let soe_nif::blocks::Block::Unknown { data, .. } = &mut file.blocks[1] else {
            panic!("fixture controller")
        };
        match case {
            0 => {
                data.pop();
            }
            1 => data[6..10].copy_from_slice(&f32::NAN.to_le_bytes()),
            _ => data[..4].copy_from_slice(&1_i32.to_le_bytes()),
        }
        assert!(
            crate::nif_effect_animation::for_shape(&file, shape).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn rnd_39_identical_keyframes_on_different_channels_do_not_share_a_material() {
    let (file, shape) = controller_file(8, 1.0);
    let mesh = file.meshes().unwrap().remove(0);
    let mut material = super::material_desc_from(&mesh);
    material.effect.as_mut().unwrap().controllers =
        crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    let original = crate::mesh::MaterialKey::from(&material);
    for channel in [0, 1, 5, 6, 7, 9] {
        material.effect.as_mut().unwrap().controllers[0].channel = channel;
        assert_ne!(
            original,
            crate::mesh::MaterialKey::from(&material),
            "channel {channel}"
        );
    }
}

#[test]
#[cfg(feature = "godot")]
fn rnd_39_lighting_waterfall_uv_tracks_sample_all_texture_layers() {
    use soe_nif::blocks::{Block, LightingShader};
    let (mut file, shape) = controller_file(8, 1.0);
    let Block::EffectShader(effect) = &file.blocks[0] else {
        panic!("fixture shader")
    };
    file.blocks[0] = Block::LightingShader(LightingShader {
        net: effect.net.clone(),
        flags1: effect.flags1,
        flags2: effect.flags2,
        ..Default::default()
    });
    let Block::Unknown { type_name, data } = &mut file.blocks[1] else {
        panic!("fixture controller")
    };
    *type_name = "BSLightingShaderPropertyFloatController".into();
    data[30..34].copy_from_slice(&22_u32.to_le_bytes());
    let tracks = crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    assert_eq!(tracks[0].channel, 8);
    let desc = crate::mesh::MaterialDesc {
        texture_transform: Some(crate::mesh::TextureTransform {
            offset: [0.25, 0.5],
            scale: [2.0, 3.0],
            controllers: tracks,
        }),
        ..Default::default()
    };
    let shader = crate::godot_layer::decal_shader(&desc);
    for texture in [
        "albedo_texture",
        "normal_texture",
        "glow_texture",
        "environment_mask",
    ] {
        assert!(
            shader.contains(&format!("texture({texture}, uv)")),
            "{texture}"
        );
    }
    assert!(shader.contains("animated_offset.y = effect_track_0"));
    let mut other = desc.clone();
    other.texture_transform.as_mut().unwrap().controllers[0].phase += 0.5;
    assert_ne!(
        crate::mesh::MaterialKey::from(&desc),
        crate::mesh::MaterialKey::from(&other)
    );
}

#[test]
fn rnd_39_authored_additive_foam_preserves_alpha_and_scroll() {
    use soe_nif::blocks::BlendFactor;
    use soe_nif::mesh::Blend;
    let (file, shape) = controller_file(8, 1.0);
    let mesh = file.meshes().unwrap().remove(0);
    let mut material = super::material_desc_from(&mesh);
    let effect = material.effect.as_mut().unwrap();
    effect.controllers = crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    effect.blend = Some(Blend {
        src: BlendFactor::SrcAlpha,
        dst: BlendFactor::One,
    });
    let shader = crate::nif_effect::shader(&material, material.effect.as_ref().unwrap());
    assert!(shader.contains("blend_add"));
    assert!(shader.contains("ALPHA = opacity;"));
    assert!(shader.contains("animated_offset.y = effect_track_0"));
    assert!(shader.contains("depth_draw_never"));
}

#[test]
fn rnd_39_authored_wrap_axes_use_godots_boolean_uniform_bitmask() {
    for (nif_mode, godot_bits) in [(0, 0), (1, 2), (2, 1), (3, 3)] {
        assert_eq!(crate::nif_effect::wrap_bits(nif_mode), godot_bits);
    }
}

#[test]
fn rnd_39_falloff_controller_degrees_become_shader_cosines() {
    use soe_nif::blocks::Block;
    let (mut file, shape) = controller_file(8, 1.0);
    let Block::Unknown { data, .. } = &mut file.blocks[1] else {
        panic!("fixture controller")
    };
    data[30..34].copy_from_slice(&1_u32.to_le_bytes());
    let Block::FloatData(keys) = &mut file.blocks[2] else {
        panic!("fixture keys")
    };
    for (key, degrees) in keys.keys.iter_mut().zip([30.0, 60.0, 90.0]) {
        key.value = degrees;
    }
    let tracks = crate::nif_effect_animation::for_shape(&file, shape).unwrap();
    assert_eq!(tracks[0].keys.keys[0].value, 30.0);
    let mut material = super::material_desc_from(&file.meshes().unwrap()[0]);
    material.effect.as_mut().unwrap().controllers = tracks;
    let shader = crate::nif_effect::shader(&material, material.effect.as_ref().unwrap());
    assert!(shader.contains("animated_falloff.x = cos(radians(effect_track_0("));
}
