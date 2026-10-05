// SPDX-License-Identifier: GPL-3.0-or-later

//! CPU graph, blend, pose and skin-matrix capacity using the host's pipeline.
//! Synthetic static spline tracks follow hkx/tests/generator_transition.rs.
//! Skin matrices reuse the nifly-based host implementation, without a GPU.

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use soe_godot_host::actor::ActorAssets;
use soe_godot_host::skinned::{skinning_transforms_into, Affine, BoneMap};
use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::behavior::{
    BehaviorGraph, BehaviorGraphData, BlenderChild, BlenderGenerator, ClipGenerator, Node, NodeId,
    NodeKind,
};
use soe_hkx::interp::{Clip, ClipLibrary, Interpreter};
use soe_hkx::pose::to_model_space_into;
use soe_hkx::skeleton::{Bone, Skeleton};
use soe_hkx::QsTransform;
use soe_nif::mesh::Skin;
use soe_nif::{BlockRef, Bound, Transform};

struct Rig {
    graph: BehaviorGraph,
    clips: ClipLibrary,
    skeleton: Skeleton,
    skins: Vec<(Skin, BoneMap)>,
}

fn clip(name: &str, x: f32, bones: usize) -> Result<Clip, String> {
    let mut data: Vec<u8> = (0..bones).flat_map(|_| [0, 1, 0, 0]).collect();
    for _ in 0..bones {
        data.extend_from_slice(&x.to_le_bytes());
    }
    Clip::from_parts(
        name,
        SplineCompressedAnimation {
            duration: 10.0,
            num_transform_tracks: bones,
            num_float_tracks: 0,
            annotation_tracks: vec![],
            num_frames: 2,
            num_blocks: 1,
            max_frames_per_block: 2,
            mask_and_quantization_size: bones as i32 * 4,
            block_duration: 10.0,
            block_inverse_duration: 0.1,
            frame_duration: 10.0,
            block_offsets: vec![0],
            float_block_offsets: vec![data.len() as u32],
            transform_offsets: vec![],
            float_offsets: vec![],
            data,
            endian: 0,
            extracted_motion: None,
        },
        &[],
    )
    .map_err(|e| e.to_string())
}

fn synthetic() -> Result<Rig, String> {
    let bones = 99;
    let skeleton = Skeleton {
        name: "PRF-28".into(),
        parent_indices: (0..bones).map(|i| if i == 0 { -1 } else { 0 }).collect(),
        bones: (0..bones)
            .map(|i| Bone {
                name: format!("bone{i}"),
                lock_translation: false,
            })
            .collect(),
        reference_pose: vec![QsTransform::IDENTITY; bones],
        reference_floats: vec![],
        float_slots: vec![],
    };
    let mut clips = ClipLibrary::new();
    clips.insert(clip("a", 1.0, bones)?);
    clips.insert(clip("b", 3.0, bones)?);
    let mut nodes: Vec<_> = ["a", "b"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| Node {
            id: NodeId(i),
            offset: i as u32,
            class: "hkbClipGenerator".into(),
            name: name.into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Clip(ClipGenerator {
                animation_name: name.into(),
                triggers: vec![],
                crop_start_amount_local_time: 0.0,
                crop_end_amount_local_time: 0.0,
                start_time: 0.0,
                playback_speed: 1.0,
                enforced_duration: 0.0,
                user_controlled_time_fraction: 0.0,
                animation_binding_index: -1,
                mode: 1,
                flags: 0,
            }),
        })
        .collect();
    nodes.push(Node {
        id: NodeId(2),
        offset: 2,
        class: "hkbBlenderGenerator".into(),
        name: "blend".into(),
        user_data: 0,
        bindings: None,
        kind: NodeKind::Blender(BlenderGenerator {
            reference_pose_weight_threshold: 0.0,
            blend_parameter: 0.0,
            min_cyclic_blend_parameter: 0.0,
            max_cyclic_blend_parameter: 1.0,
            index_of_sync_master_child: -1,
            flags: 0,
            subtract_last_child: false,
            children: (0..2)
                .map(|i| BlenderChild {
                    generator: Some(NodeId(i)),
                    bone_weights: None,
                    bone_weights_pointer: None,
                    weight: 0.5,
                    world_from_model_weight: 1.0,
                    bindings: None,
                })
                .collect(),
            bone_weight_masks: HashMap::new(),
        }),
    });
    let graph = BehaviorGraph {
        name: "PRF-28".into(),
        variable_mode: 1,
        root: Some(NodeId(2)),
        nodes,
        data: BehaviorGraphData {
            attribute_defaults: vec![],
            variable_infos: vec![],
            character_property_infos: vec![],
            event_infos: vec![],
            word_min_variable_values: vec![],
            word_max_variable_values: vec![],
            variable_initial_values: None,
            event_names: vec![],
            attribute_names: vec![],
            variable_names: vec![],
            character_property_names: vec![],
        },
    };
    let skin = Skin {
        skeleton_root: BlockRef::NONE,
        skeleton_root_name: "PRF-28".into(),
        transform: Transform::IDENTITY,
        bones: skeleton
            .bones
            .iter()
            .map(|bone| soe_nif::mesh::Bone {
                name: bone.name.clone(),
                node: BlockRef::NONE,
                inverse_bind: Transform::IDENTITY,
                bound: Bound::default(),
            })
            .collect(),
        body_parts: vec![],
    };
    let map = BoneMap::build(&skin.bones, &skeleton);
    Ok(Rig {
        graph,
        clips,
        skeleton,
        skins: vec![(skin, map)],
    })
}

fn installed() -> Result<Rig, String> {
    let data = super::data_dir()?;
    let mut vfs = soe_bsa::Vfs::new();
    for name in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&data.join(name))
            .map_err(|e| e.to_string())?;
    }
    let order = std::sync::Arc::new(crate::read_order_threads(&data, 1)?);
    let (game, actors) = super::installed_actors(order, 1)?;
    let actor = actors.first().copied().ok_or("Whiterun actor absent")?;
    let assets = ActorAssets::load_for_actor(&mut game.world_mut(), actor, &vfs)
        .map_err(|e| e.to_string())?;
    println!(
        "ANIMATION_ACTOR actor={actor} race={:?} female={:?} skeleton={} behavior={}",
        assets.actor_race, assets.actor_female, assets.skeleton_path, assets.behavior_path
    );
    println!(
        "ANIMATION_ASSETS missing_clips={} unresolved_references={}",
        assets.missing_clips.len(),
        assets.unresolved_references.len()
    );
    Ok(Rig {
        graph: assets.graph,
        clips: assets.clips,
        skeleton: assets.skeleton,
        skins: assets
            .surfaces
            .into_iter()
            .map(|surface| (surface.skin, surface.map))
            .collect(),
    })
}

struct Actor<'a> {
    vm: Interpreter<'a>,
    model: Vec<QsTransform>,
    matrices: Vec<Affine>,
}

impl<'a> Actor<'a> {
    fn new(rig: &'a Rig) -> Self {
        let mut vm = Interpreter::new(&rig.graph, &rig.clips, rig.skeleton.reference_pose.clone());
        vm.variables.set_by_name("Speed", 1.0);
        vm.send_event_named("moveStart");
        Self {
            vm,
            model: Vec::new(),
            matrices: Vec::new(),
        }
    }

    fn step(&mut self, rig: &Rig) -> Result<(), String> {
        self.vm.update(1.0 / 60.0);
        self.vm.take_events();
        to_model_space_into(&rig.skeleton, self.vm.pose(), &mut self.model)
            .map_err(|e| e.to_string())?;
        for (skin, map) in &rig.skins {
            skinning_transforms_into(map, skin, &self.model, &mut self.matrices)
                .map_err(|e| e.to_string())?;
            black_box(&self.matrices);
        }
        Ok(())
    }

    fn verify(&self, rig: &Rig) -> Result<(), String> {
        if self.vm.frame() < 2
            || self.model.len() != rig.skeleton.bones.len()
            || self.matrices.is_empty()
            || self
                .model
                .iter()
                .flat_map(|q| q.translation.iter().chain(&q.rotation).chain(&q.scale))
                .any(|v| !v.is_finite())
            || self.matrices.iter().any(|matrix| {
                matrix
                    .origin
                    .iter()
                    .chain(matrix.basis.iter().flatten())
                    .any(|v| !v.is_finite())
            })
        {
            return Err("animation produced invalid or empty skinning work".into());
        }
        Ok(())
    }
}

pub(super) fn run(config: &super::Config) -> Result<(), String> {
    let rig = if config.world == "synthetic" {
        synthetic()?
    } else {
        installed()?
    };
    println!(
        "ANIMATION_WORK bones={} graph_nodes={} clips={} skin_matrices={} gpu_skinning=unmeasured",
        rig.skeleton.bones.len(),
        rig.graph.nodes.len(),
        rig.clips.len(),
        rig.skins
            .iter()
            .map(|(skin, _)| skin.bones.len())
            .sum::<usize>()
    );
    super::sweep(config, |count| {
        let mut actors: Vec<_> = (0..count).map(|_| Actor::new(&rig)).collect();
        for _ in 0..config.warmup {
            for actor in &mut actors {
                actor.step(&rig)?;
            }
        }
        let mut samples = Vec::with_capacity(config.samples);
        for _ in 0..config.samples {
            let started = Instant::now();
            for actor in &mut actors {
                actor.step(&rig)?;
            }
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        for actor in &actors {
            actor.verify(&rig)?;
        }
        println!("ANIMATION_UNSUPPORTED {:?}", actors[0].vm.unsupported());
        Ok(samples)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_28_synthetic_animation_blends_99_bones_and_skins_each_actor() {
        let rig = synthetic().unwrap();
        let mut actors: Vec<_> = (0..4).map(|_| Actor::new(&rig)).collect();
        for _ in 0..3 {
            for actor in &mut actors {
                actor.step(&rig).unwrap();
            }
        }
        for actor in &actors {
            actor.verify(&rig).unwrap();
            assert_eq!(actor.vm.pose()[0].translation[0], 2.0);
            assert_eq!(actor.matrices.len(), 99);
            assert_eq!(
                actor.vm.unsupported(),
                &soe_hkx::interp::Unsupported::default()
            );
        }
    }

    #[test]
    fn prf_28_installed_animation_runs_graph_and_skin_pipeline() {
        if super::super::data_dir().is_err() {
            return;
        }
        let rig = installed().unwrap();
        let mut actor = Actor::new(&rig);
        for _ in 0..3 {
            actor.step(&rig).unwrap();
        }
        actor.verify(&rig).unwrap();
        assert!(!rig.clips.is_empty());
    }
}
