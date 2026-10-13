// SPDX-License-Identifier: GPL-3.0-or-later

//! Door extents cover the authored closed pose and Open and Close child sweeps.

pub fn radius(loaded: &soe_godot_host::data_source::LoadedCell, position: [f32; 3]) -> f32 {
    let hinge =
        soe_godot_host::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
    let mut radius = 0.0_f32;
    for surface in &loaded.surfaces {
        assert!(surface.instance_transform.is_none());
        let mut poses = vec![None];
        if let Some(animation) = &surface.animation {
            for name in ["Open", "Close"] {
                if let Some(duration) = animation.asset.sequence_duration(name) {
                    for step in 0..=64 {
                        poses.push(Some(
                            animation
                                .asset
                                .sample(duration * step as f32 / 64.0, Some(name)),
                        ));
                    }
                }
            }
        }
        for pose in poses {
            let transform = surface.animation.as_ref().map(|animation| {
                let pose = pose.unwrap_or_else(|| animation.asset.sample(0.0, None));
                let shape = pose.transforms[animation.block];
                soe_godot_host::skinned::Affine::from_nif(&animation.placement.then(&shape))
                    .to_godot()
            });
            for point in &surface.arrays.positions {
                let point = transform.map_or(*point, |transform| transform.apply(*point));
                radius = radius.max(
                    ((point[0] - hinge.x).powi(2)
                        + (point[1] - hinge.y).powi(2)
                        + (point[2] - hinge.z).powi(2))
                    .sqrt(),
                );
            }
        }
    }
    radius
}
