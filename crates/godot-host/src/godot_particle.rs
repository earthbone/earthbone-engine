// SPDX-License-Identifier: GPL-3.0-or-later

//! Draw a named NIF particle emitter through one MultiMesh RID. Particle
//! timing comes from `soe-nif`, whose stream layout follows nifxml and
//! nifly `Particles.cpp`. The Godot node only uploads the live positions.

use std::path::Path;

use godot::classes::base_material_3d::{CullMode, ShadingMode, Transparency};
use godot::classes::multi_mesh::TransformFormat;
use godot::classes::{INode3D, MultiMesh, Node3D, QuadMesh, RenderingServer, StandardMaterial3D};
use godot::prelude::*;
use soe_nif::particle::Timeline;

use crate::units::SkyrimPos;

/// A named emitter loaded from a NIF, drawn as one server-side MultiMesh.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct ParticleEmitterNode {
    base: Base<Node3D>,
    #[export]
    playing: bool,
    timeline: Option<Timeline>,
    multi: Option<Gd<MultiMesh>>,
    instance: Option<Rid>,
}

#[godot_api]
impl INode3D for ParticleEmitterNode {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            base,
            playing: true,
            timeline: None,
            multi: None,
            instance: None,
        }
    }

    fn process(&mut self, delta: f64) {
        if self.playing {
            self.advance(delta as f32);
        }
    }

    fn exit_tree(&mut self) {
        self.clear();
    }
}

#[godot_api]
impl ParticleEmitterNode {
    /// Load one emitter by its `NiPSysModifier` name from a NIF path.
    /// The node must be inside a 3D world before calling this method.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn load_nif(&mut self, nif_path: GString, emitter_name: GString) -> bool {
        let path = nif_path.to_string();
        let bytes = match std::fs::read(Path::new(&path)) {
            Ok(bytes) => bytes,
            Err(error) => {
                soe_trace::error!(target: "host::particle", system = "host::particle", "ParticleEmitterNode: {path}: {error}");
                return false;
            }
        };
        let file = match soe_nif::parse(&bytes) {
            Ok(file) => file,
            Err(error) => {
                soe_trace::error!(target: "host::particle", system = "host::particle", "ParticleEmitterNode: {path}: {error}");
                return false;
            }
        };
        let emitters = match file.particle_emitters() {
            Ok(emitters) => emitters,
            Err(error) => {
                soe_trace::error!(target: "host::particle", system = "host::particle", "ParticleEmitterNode: {path}: {error}");
                return false;
            }
        };
        let name = emitter_name.to_string();
        let Some(emitter) = emitters.into_iter().find(|e| e.name == name) else {
            soe_trace::error!(target: "host::particle", system = "host::particle", "ParticleEmitterNode: {path}: no emitter named {name}");
            return false;
        };
        let Some(scenario) = self.base().get_world_3d().map(|world| world.get_scenario()) else {
            soe_trace::error!(target: "host::particle", system = "host::particle", "ParticleEmitterNode: add the node to a 3D world first");
            return false;
        };
        self.clear();
        let mut material = StandardMaterial3D::new_gd();
        material.set_albedo(Color::from_rgba(
            emitter.color[0],
            emitter.color[1],
            emitter.color[2],
            emitter.color[3],
        ));
        material.set_transparency(Transparency::ALPHA);
        material.set_cull_mode(CullMode::DISABLED);
        material.set_shading_mode(ShadingMode::UNSHADED);
        let mut quad = QuadMesh::new_gd();
        let size = SkyrimPos::new(emitter.radius, 0.0, 0.0).to_godot().x.abs() * 2.0;
        quad.set_size(Vector2::new(size, size));
        quad.set_material(&material);
        let mut multi = MultiMesh::new_gd();
        multi.set_transform_format(TransformFormat::TRANSFORM_3D);
        multi.set_mesh(&quad);
        multi.set_instance_count(emitter.capacity as i32);
        multi.set_visible_instance_count(0);
        let mut server = RenderingServer::singleton();
        soe_trace::count(soe_trace::Operation::RenderInstances, 1);
        let instance = server.instance_create2(multi.get_rid(), scenario);
        server.instance_set_transform(instance, self.base().get_global_transform());
        self.timeline = Some(Timeline::new(emitter));
        self.multi = Some(multi);
        self.instance = Some(instance);
        true
    }

    /// Advance births and deaths and upload the surviving particle positions.
    #[func]
    pub fn advance(&mut self, delta: f32) {
        let Some(timeline) = self.timeline.as_mut() else {
            return;
        };
        timeline.advance(delta);
        let Some(multi) = self.multi.as_mut() else {
            return;
        };
        for (index, particle) in timeline.particles().iter().enumerate() {
            let [x, y, z] = particle.position;
            let p = SkyrimPos::new(x, y, z).to_godot();
            let transform = Transform3D::new(Basis::IDENTITY, Vector3::new(p.x, p.y, p.z));
            multi.set_instance_transform(index as i32, transform);
        }
        multi.set_visible_instance_count(timeline.alive() as i32);
        if let Some(instance) = self.instance {
            RenderingServer::singleton()
                .instance_set_transform(instance, self.base().get_global_transform());
        }
    }

    /// Number of currently drawn particles.
    #[func]
    pub fn particle_count(&self) -> i32 {
        self.timeline
            .as_ref()
            .map_or(0, |state| state.alive() as i32)
    }

    /// Total births since the last successful load.
    #[func]
    pub fn emitted_count(&self) -> i64 {
        self.timeline
            .as_ref()
            .map_or(0, |state| state.emitted() as i64)
    }

    /// Number of instances the MultiMesh currently makes visible.
    #[func]
    pub fn rendered_count(&self) -> i32 {
        self.multi
            .as_ref()
            .map_or(0, |multi| multi.get_visible_instance_count())
    }

    fn clear(&mut self) {
        if let Some(instance) = self.instance.take() {
            RenderingServer::singleton().free_rid(instance);
        }
        self.multi = None;
        self.timeline = None;
    }
}
