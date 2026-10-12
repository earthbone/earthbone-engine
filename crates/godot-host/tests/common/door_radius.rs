// SPDX-License-Identifier: GPL-3.0-or-later

//! Door extents follow the RND-23 initial model-space pose.

pub fn radius(loaded: &soe_godot_host::data_source::LoadedCell, position: [f32; 3]) -> f32 {
    let hinge =
        soe_godot_host::units::SkyrimPos::new(position[0], position[1], position[2]).to_godot();
    loaded
        .surfaces
        .iter()
        .flat_map(|surface| {
            assert!(surface.instance_transform.is_none());
            let transform = surface.animation.as_ref().map(|animation| {
                let pose = animation.asset.sample(0.0, None);
                let shape = pose.transforms[animation.block];
                soe_godot_host::skinned::Affine::from_nif(&animation.placement.then(&shape))
                    .to_godot()
            });
            surface.arrays.positions.iter().map(move |point| {
                let point = transform.map_or(*point, |transform| transform.apply(*point));
                ((point[0] - hinge.x).powi(2)
                    + (point[1] - hinge.y).powi(2)
                    + (point[2] - hinge.z).powi(2))
                .sqrt()
            })
        })
        .fold(0.0, f32::max)
}
