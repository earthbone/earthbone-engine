// SPDX-License-Identifier: GPL-3.0-or-later

//! Retained material representatives rendered beneath the startup loading cover.

use std::thread::JoinHandle;
use std::time::Duration;

use godot::classes::ArrayMesh;
use godot::prelude::*;

use super::{array_mesh_from, material_for, CellStreamerNode, MaterialMap};

#[derive(Default)]
pub(super) struct State {
    worker: Option<JoinHandle<(crate::tree_warmup::Prepared, Duration)>>,
    fingerprint: Option<[u8; 32]>,
    meshes: Vec<Gd<ArrayMesh>>,
    models: usize,
    failures: Vec<String>,
    prepare_ms: f64,
    publish_ms: f64,
    mesh_bytes: usize,
}

impl Drop for State {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[godot_api(secondary)]
impl CellStreamerNode {
    #[func]
    pub fn begin_tree_pipeline_warmup(&mut self) -> bool {
        let Some(source) = self.data_source.clone() else {
            return false;
        };
        let fingerprint = source.lod_load_order_fingerprint();
        if self.pipeline_warmup.fingerprint == Some(fingerprint)
            && (!self.pipeline_warmup.meshes.is_empty() || self.pipeline_warmup.worker.is_some())
        {
            return true;
        }
        self.pipeline_warmup = State::default();
        self.pipeline_warmup.fingerprint = Some(fingerprint);
        match std::thread::Builder::new()
            .name("tree-warmup".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let prepared = crate::tree_warmup::prepare(&source);
                (prepared, started.elapsed())
            }) {
            Ok(worker) => {
                self.pipeline_warmup.worker = Some(worker);
                true
            }
            Err(error) => {
                soe_trace::error!(target: "host::warmup", system = "load", "tree material preparation: {error}");
                false
            }
        }
    }

    #[func]
    pub fn poll_tree_pipeline_warmup(&mut self) -> i32 {
        if !self
            .pipeline_warmup
            .worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            return if self.pipeline_warmup.worker.is_some() {
                0
            } else if self.pipeline_warmup.meshes.is_empty() {
                -1
            } else {
                1
            };
        }
        let Some(worker) = self.pipeline_warmup.worker.take() else {
            return -1;
        };
        let Ok((prepared, elapsed)) = worker.join() else {
            return -1;
        };
        let started = std::time::Instant::now();
        let mut materials = MaterialMap {
            shaders: self.materials.shaders.clone(),
            ..MaterialMap::default()
        };
        self.pipeline_warmup.models = prepared.models;
        self.pipeline_warmup.failures = prepared.failures;
        self.pipeline_warmup.prepare_ms = elapsed.as_secs_f64() * 1000.0;
        for (arrays, textures) in prepared.surfaces {
            let material = material_for(&mut materials, &arrays.material, &textures);
            let mut mesh = array_mesh_from(&arrays);
            mesh.surface_set_material(0, &material);
            self.pipeline_warmup.mesh_bytes += arrays.mesh_byte_count();
            self.pipeline_warmup.meshes.push(mesh);
        }
        self.materials.shaders = materials.shaders;
        self.pipeline_warmup.publish_ms = started.elapsed().as_secs_f64() * 1000.0;
        1
    }

    #[func]
    pub fn tree_pipeline_warmup_meshes(&self) -> Array<Gd<ArrayMesh>> {
        self.pipeline_warmup.meshes.iter().cloned().collect()
    }

    #[func]
    pub fn tree_pipeline_warmup_stats(&self) -> VarDictionary {
        let failures = self
            .pipeline_warmup
            .failures
            .iter()
            .map(|row| GString::from(row.as_str()))
            .collect::<PackedStringArray>();
        vdict! { "models" => self.pipeline_warmup.models as i64,
        "meshes" => self.pipeline_warmup.meshes.len() as i64,
        "mesh_bytes" => self.pipeline_warmup.mesh_bytes as i64,
        "prepare_ms" => self.pipeline_warmup.prepare_ms,
        "publish_ms" => self.pipeline_warmup.publish_ms,
        "failures" => &failures }
    }
}
