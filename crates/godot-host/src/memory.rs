// SPDX-License-Identifier: GPL-3.0-or-later

//! Retained render payload attribution without GPU readback. Mesh attribute
//! and image payload accounting follows Mudcrab PR 224's CPU asset report.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::data_source::{SurfaceTextures, TextureData};
use crate::mesh::SurfaceArrays;

// Ported from Mudcrab (github.com/Mudcrab-Team/mudcrab, crates/engine/src/profiling.rs
// at PR 224), MIT License, Copyright (c) the Mudcrab contributors
/// Counts CPU payloads once per owner, separating mesh attributes and indices.
#[derive(Default)]
pub(crate) struct AssetMemory {
    pub values: BTreeMap<String, u64>,
    images: HashSet<usize>,
}

impl AssetMemory {
    pub fn add(&mut self, name: &str, value: usize) {
        *self.values.entry(name.to_owned()).or_default() += value as u64;
    }

    pub fn mesh(&mut self, arrays: &SurfaceArrays) {
        let indices = std::mem::size_of_val(arrays.indices.as_slice());
        self.add("cpu_mesh_count", 1);
        self.add("cpu_mesh_allocation_bytes", arrays.allocation_bytes());
        self.add("cpu_mesh_vertex_bytes", arrays.mesh_byte_count() - indices);
        self.add("cpu_mesh_index_bytes", indices);
    }

    pub fn textures(&mut self, textures: &SurfaceTextures) {
        for image in [
            &textures.diffuse,
            &textures.normal,
            &textures.glow,
            &textures.greyscale,
            &textures.environment,
            &textures.environment_mask,
            &textures.terrain_noise,
        ]
        .into_iter()
        .chain(textures.water_noise.iter())
        .chain([&textures.water_flow])
        .flatten()
        {
            self.image(image);
        }
    }

    pub fn image(&mut self, image: &Arc<TextureData>) {
        if self.images.insert(Arc::as_ptr(image) as usize) {
            self.add("cpu_image_count", 1);
            self.add("cpu_image_allocation_bytes", image.retained_bytes());
            self.add("cpu_image_bytes", image.payload_bytes());
            self.image_format(image);
        }
    }

    fn image_format(&mut self, image: &TextureData) {
        match image {
            TextureData::Cubemap(faces) => {
                for face in faces.iter() {
                    self.image_format(face);
                }
            }
            TextureData::Compressed { format, bytes, .. } => {
                self.add(&format!("cpu_image_{format:?}_count"), 1);
                self.add(&format!("cpu_image_{format:?}_bytes"), bytes.len());
            }
            TextureData::Rgba8(image) => {
                self.add("cpu_image_Rgba8_count", 1);
                self.add("cpu_image_Rgba8_bytes", image.rgba.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_image_payload_is_counted_once_per_allocation() {
        let image = Arc::new(TextureData::Rgba8(soe_dds::Image {
            width: 2,
            height: 2,
            depth: 1,
            rgba: vec![0; 16],
        }));
        let mut report = AssetMemory::default();
        report.image(&image);
        report.image(&Arc::clone(&image));
        assert_eq!(report.values["cpu_image_count"], 1);
        assert_eq!(report.values["cpu_image_bytes"], 16);
        report.image(&Arc::new((*image).clone()));
        assert_eq!(report.values["cpu_image_count"], 2);
        assert_eq!(report.values["cpu_image_bytes"], 32);
    }
}
