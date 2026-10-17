// SPDX-License-Identifier: GPL-3.0-or-later

//! Profile-selected tree material and vertex layouts for load-time rendering.
//! Godot's pipeline_compilations.rst requires real material instances before
//! gameplay. Reuse the production NIF builder and keep its authored cutoffs.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use crate::data_source::{checker_albedo, model_for, DataSource, SurfaceTextures, TextureData};
use crate::mesh::{build_surface, Aabb, MaterialKey, MeshOptions, SurfaceArrays};
use crate::units::GodotPos;

#[derive(Default)]
pub(crate) struct Prepared {
    pub surfaces: Vec<(SurfaceArrays, SurfaceTextures)>,
    pub models: usize,
    pub failures: Vec<String>,
}

pub(crate) fn prepare(source: &DataSource) -> Prepared {
    let _span = soe_trace::Span::new("host.tree_warmup_prepare", "host");
    let models: BTreeSet<_> = source
        .order()
        .winners_of(soe_esm::Tag::new(b"TREE"))
        .filter_map(|(_, record)| model_for(record))
        .map(|path| path.replace('\\', "/").to_ascii_lowercase())
        .collect();
    let mut result = Prepared {
        models: models.len(),
        ..Prepared::default()
    };
    let mut seen = HashSet::new();
    for model in models {
        match source.item_preview_surfaces(&model) {
            Ok(surfaces) => {
                for (arrays, textures) in surfaces {
                    let layout = layout(&arrays);
                    let texture_slots = [
                        textures.diffuse.is_some(),
                        textures.normal.is_some(),
                        textures.glow.is_some(),
                        textures.environment.is_some(),
                        textures.environment_mask.is_some(),
                        textures.greyscale.is_some(),
                    ];
                    if seen.insert((MaterialKey::from(&arrays.material), layout, texture_slots)) {
                        result
                            .surfaces
                            .push((representative(arrays), placeholders(&textures)));
                    }
                }
            }
            Err(error) => result.failures.push(format!("{model}: {error}")),
        }
    }
    let types = BTreeMap::from([(
        0,
        crate::tree_lod::TreeType {
            width: 64.0,
            height: 128.0,
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
        },
    )]);
    let references = [crate::tree_lod::TreeReference {
        tree_type: 0,
        position: [0.0; 3],
        rotation: 0.0,
        scale: 1.0,
        raw_form_id: 0,
    }];
    match crate::tree_lod::build_billboards(&types, &references, "warmup/tree-billboard.dds") {
        Ok(mesh) => match build_surface(&mesh, MeshOptions::default()) {
            Ok(arrays) => result.surfaces.push((
                representative(arrays),
                SurfaceTextures {
                    diffuse: Some(checker_albedo()),
                    ..SurfaceTextures::default()
                },
            )),
            Err(error) => result.failures.push(format!("billboard surface: {error}")),
        },
        Err(error) => result.failures.push(format!("billboard geometry: {error}")),
    }
    result
}

fn layout(arrays: &SurfaceArrays) -> (bool, bool, bool, bool, usize) {
    (
        !arrays.normals.is_empty(),
        !arrays.tangents.is_empty(),
        !arrays.uvs.is_empty(),
        !arrays.colors.is_empty(),
        arrays
            .bones
            .len()
            .checked_div(arrays.positions.len())
            .unwrap_or_default(),
    )
}

fn representative(mut arrays: SurfaceArrays) -> SurfaceArrays {
    let (normals, tangents, uvs, colors, influences) = layout(&arrays);
    arrays.positions = vec![[0.0, 0.0, 0.0], [0.04, 0.0, 0.0], [0.0, 0.04, 0.0]];
    arrays.normals = if normals {
        vec![[0.0, 0.0, 1.0]; 3]
    } else {
        Vec::new()
    };
    arrays.tangents = if tangents {
        [1.0, 0.0, 0.0, 1.0].repeat(3)
    } else {
        Vec::new()
    };
    arrays.uvs = if uvs {
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
    } else {
        Vec::new()
    };
    arrays.colors = if colors {
        vec![[1.0; 4]; 3]
    } else {
        Vec::new()
    };
    arrays.indices = vec![0, 2, 1];
    arrays.bones = vec![0; 3 * influences];
    arrays.weights = vec![0.0; 3 * influences];
    if influences > 0 {
        for weights in arrays.weights.chunks_mut(influences) {
            weights[0] = 1.0;
        }
    }
    arrays.bounds = Aabb {
        min: GodotPos::new(0.0, 0.0, 0.0),
        max: GodotPos::new(0.04, 0.04, 0.0),
    };
    arrays
}

fn placeholder(texture: Option<&Arc<TextureData>>) -> Option<Arc<TextureData>> {
    texture.map(|texture| {
        if matches!(texture.as_ref(), TextureData::Cubemap(_)) {
            Arc::new(TextureData::Cubemap(Box::new(std::array::from_fn(|_| {
                checker_albedo().as_ref().clone()
            }))))
        } else {
            checker_albedo()
        }
    })
}

fn placeholders(textures: &SurfaceTextures) -> SurfaceTextures {
    SurfaceTextures {
        diffuse: placeholder(textures.diffuse.as_ref()),
        normal: placeholder(textures.normal.as_ref()),
        glow: placeholder(textures.glow.as_ref()),
        environment: placeholder(textures.environment.as_ref()),
        environment_mask: placeholder(textures.environment_mask.as_ref()),
        greyscale: placeholder(textures.greyscale.as_ref()),
        terrain_noise: placeholder(textures.terrain_noise.as_ref()),
        water_noise: textures
            .water_noise
            .each_ref()
            .map(|texture| placeholder(texture.as_ref())),
        water_flow: placeholder(textures.water_flow.as_ref()),
    }
}
