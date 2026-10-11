// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Object animation material ownership and GPU skin poses through the public
//! host API. Fixtures follow nifly Animation.hpp and nif.xml independently.

use std::sync::Arc;

use soe_godot_host::mesh::{EffectMaterial, MaterialDesc};
use soe_godot_host::object_animation::AnimatedSurface;
use soe_nif::mesh::{Bone, Skin};
use soe_nif::object_animation::{ObjectAnimation, ObjectPose, PropertyChannel, PropertyValue};
use soe_nif::{BlockRef, Bound, Transform, Vec3};

#[path = "../../nif/tests/common/mod.rs"]
mod common;

fn surface() -> AnimatedSurface {
    let mut w = common::NifWriter::new(100);
    let node = w.ni_node("Root", [0.0; 3], common::IDENTITY, 1.0, &[]);
    let root = w.add("NiNode", node);
    w.roots = vec![root];
    let file = soe_nif::parse(&w.finish()).unwrap();
    AnimatedSurface {
        asset: Arc::new(ObjectAnimation::from_file(&file).unwrap()),
        block: 0,
        shader: Some(1),
        skin: None,
        placement: Transform::IDENTITY,
        uv_offset: [0.0; 2],
        uv_scale: [1.0; 2],
        alpha: 1.0,
    }
}

#[test]
fn rnd_23_skin_bones_use_model_pose_and_inverse_bind_in_mesh_order() {
    let mut surface = surface();
    surface.skin = Some(Skin {
        skeleton_root: BlockRef(0),
        skeleton_root_name: "Root".into(),
        transform: Transform::IDENTITY,
        bones: vec![Bone {
            name: "Bone".into(),
            node: BlockRef(1),
            inverse_bind: Transform {
                translation: Vec3::new(-10.0, 0.0, 0.0),
                ..Transform::IDENTITY
            },
            bound: Bound::default(),
        }],
        body_parts: vec![],
    });
    let mut pose = ObjectPose {
        transforms: vec![
            Transform::IDENTITY,
            Transform {
                translation: Vec3::new(10.0, 0.0, 0.0),
                ..Transform::IDENTITY
            },
        ],
        properties: vec![],
    };
    assert_eq!(
        surface.skin_transforms(&pose)[0],
        soe_godot_host::skinned::Affine::IDENTITY
    );
    pose.transforms[1].translation.z = 70.0;
    assert!(
        (surface.skin_transforms(&pose)[0].origin[1]
            - 70.0 * soe_godot_host::units::METRES_PER_UNIT)
            .abs()
            < 1e-6
    );
}

#[test]
fn rnd_23_material_animation_keeps_other_placements_and_properties_unchanged() {
    let surface = surface();
    let base = MaterialDesc {
        effect: Some(EffectMaterial {
            params: soe_nif::mesh::EffectParams::default(),
            blend: None,
            alpha_test: None,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            controllers: Vec::new(),
        }),
        ..Default::default()
    };
    let mut pose = ObjectPose {
        transforms: vec![],
        properties: vec![],
    };
    for (channel, value) in [
        (PropertyChannel::Alpha, 0.25),
        (PropertyChannel::UOffset, 0.5),
        (PropertyChannel::VOffset, 0.75),
        (PropertyChannel::EmissiveMultiple, 2.0),
    ] {
        pose.properties.push(PropertyValue {
            target: 1,
            channel,
            value: Vec3::new(value, 0.0, 0.0),
        });
    }
    pose.properties.push(PropertyValue {
        target: 2,
        channel: PropertyChannel::Alpha,
        value: Vec3::ZERO,
    });
    let sampled = surface.material(&base, &pose);
    let effect = sampled.effect.unwrap();
    assert_eq!(effect.params.base_color[3], 0.25);
    assert_eq!(effect.uv_offset, [0.5, 0.75]);
    assert_eq!(effect.params.base_color_scale, 2.0);
    assert_eq!(
        surface.material(
            &base,
            &ObjectPose {
                transforms: vec![],
                properties: vec![]
            }
        ),
        base
    );
}

#[test]
fn rnd_23_animated_objects_stay_out_of_stationary_occluders_and_instancing() {
    use soe_godot_host::data_source::{LoadedSurface, SurfaceTextures};
    use soe_godot_host::mesh::{build_surface, MeshData, MeshOptions};

    let arrays = build_surface(
        &MeshData {
            positions: vec![[0.0; 3], [70.0, 0.0, 0.0], [0.0, 70.0, 0.0]],
            indices: vec![0, 1, 2],
            ..Default::default()
        },
        MeshOptions::default(),
    )
    .unwrap();
    let mut loaded = LoadedSurface {
        reference: soe_esm::GlobalFormId { file: 0, object: 1 },
        arrays,
        textures: SurfaceTextures::default(),
        animation: Some(surface()),
        placement: soe_physics::Iso::IDENTITY,
        instance_transform: None,
        mesh_identity: None,
    };
    assert!(soe_godot_host::batch::cell_occluder_mesh(&[loaded.clone()])
        .0
        .is_empty());
    loaded.mesh_identity = Some(Arc::from("animated-object"));
    loaded.instance_transform = Some(soe_godot_host::units::placement_to_godot(
        common::IDENTITY,
        [0.0; 3],
        1.0,
    ));
    let surfaces = vec![loaded.clone(); soe_godot_host::batch::INSTANCE_MIN_PLACEMENTS];
    let (groups, remainder) = soe_godot_host::batch::instancing_candidates(&surfaces);
    assert!(groups.is_empty());
    assert_eq!(remainder.len(), surfaces.len());
    loaded.animation = None;
    assert_eq!(
        soe_godot_host::batch::cell_occluder_mesh(&[loaded]).1,
        vec![0, 2, 1]
    );
}

#[test]
fn rnd_23_installed_cell_loader_retains_banner_and_activation_tracks() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    for path in [
        "clutter/upperclass/upperchest01.nif",
        "architecture/farmhouse/lumbermill01waterwheel01.nif",
        "clutter/nightingale/nightingalebanneranim01.nif",
    ] {
        let surfaces = source.animated_model_surfaces(path).unwrap();
        assert!(!surfaces.is_empty(), "{path}");
        assert!(
            surfaces.iter().all(|s| s.animation.is_some()
                && s.instance_transform.is_none()
                && s.mesh_identity.is_none()),
            "{path} stays outside static batches"
        );
        let animation = surfaces[0].animation.as_ref().unwrap();
        let names: Vec<_> = animation.asset.sequence_names().collect();
        println!("rnd_23 {path}: sequences {names:?}");
        assert!(
            std::iter::once(None)
                .chain(names.into_iter().map(Some))
                .any(|name| animation.asset.sample(0.1, name) != animation.asset.sample(0.7, name)),
            "{path} poses differ"
        );
    }
}
