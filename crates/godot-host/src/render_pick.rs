// SPDX-License-Identifier: GPL-3.0-or-later

//! Console geometry queries reuse the physics segment-triangle intersection.
//! Godot Aabb bounds reject meshes before testing their actual triangles.

use godot::prelude::*;
use soe_esm::GlobalFormId;

pub(crate) type Pick = (f32, GlobalFormId);

pub(crate) fn nearer(best: &mut Option<Pick>, candidate: Pick) {
    if best.is_none_or(|old| {
        candidate
            .0
            .total_cmp(&old.0)
            .then(candidate.1.cmp(&old.1))
            .is_lt()
    }) {
        *best = Some(candidate);
    }
}

pub(crate) fn posed_vertices(
    positions: &[[f32; 3]],
    bones: &[i32],
    weights: &[f32],
    transforms: &[Transform3D],
) -> Vec<Vector3> {
    positions
        .iter()
        .enumerate()
        .map(|(vertex, &[x, y, z])| {
            let point = Vector3::new(x, y, z);
            let mut transformed = Vector3::ZERO;
            let mut total = 0.0;
            for influence in 0..4 {
                let slot = vertex * 4 + influence;
                let (Some(&bone), Some(&weight)) = (bones.get(slot), weights.get(slot)) else {
                    continue;
                };
                let Some(transform) = usize::try_from(bone)
                    .ok()
                    .and_then(|index| transforms.get(index))
                else {
                    continue;
                };
                transformed += (*transform * point) * weight;
                total += weight;
            }
            if total > 0.0 {
                transformed
            } else {
                point
            }
        })
        .collect()
}

pub(crate) fn surface_hit(
    from: Vector3,
    to: Vector3,
    positions: &[Vector3],
    indices: Option<&[i32]>,
) -> Option<(f32, usize)> {
    let first = *positions.first()?;
    let mut bounds = Aabb::new(first, Vector3::ZERO);
    for &point in positions.iter().skip(1) {
        bounds = bounds.expand(point);
    }
    if !bounds.intersects_segment(from, to) {
        return None;
    }
    let vector = |p: Vector3| soe_physics::Vec3::new(p.x, p.y, p.z);
    let triangle_count = indices.map_or(positions.len(), <[i32]>::len) / 3;
    let mut best: Option<(f32, usize)> = None;
    for triangle in 0..triangle_count {
        let point = |corner: usize| {
            let slot = triangle * 3 + corner;
            let vertex = indices.map_or(Some(slot), |values| {
                usize::try_from(*values.get(slot)?).ok()
            })?;
            positions.get(vertex).copied().map(vector)
        };
        let (Some(a), Some(b), Some(c)) = (point(0), point(1), point(2)) else {
            continue;
        };
        if let Some(fraction) = soe_physics::world::ray_triangle(vector(from), vector(to), a, b, c)
        {
            if best.is_none_or(|old| fraction < old.0) {
                best = Some((fraction, triangle));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r4_console_surface_pick_reaches_a_creature_above_its_capsule() {
        let vertices = [
            Vector3::new(-1.0, 3.0, 0.0),
            Vector3::new(1.0, 3.0, 0.0),
            Vector3::new(0.0, 4.0, 0.0),
        ];
        let hit = surface_hit(
            Vector3::new(0.0, 3.5, 1.0),
            Vector3::new(0.0, 3.5, -1.0),
            &vertices,
            Some(&[0, 1, 2]),
        );
        assert_eq!(hit, Some((0.5, 0)));
        assert!(surface_hit(
            Vector3::new(2.0, 3.5, 1.0),
            Vector3::new(2.0, 3.5, -1.0),
            &vertices,
            Some(&[0, 1, 2])
        )
        .is_none());
    }

    #[test]
    fn r4_console_geometry_keeps_the_nearest_reference_and_stable_ties() {
        let mut best = None;
        let id = |object| GlobalFormId { file: 0, object };
        for candidate in [(0.7, id(3)), (0.2, id(2)), (0.2, id(1)), (0.8, id(0))] {
            nearer(&mut best, candidate);
        }
        assert_eq!(best, Some((0.2, id(1))));
    }
    #[test]
    fn r4_console_pick_follows_uploaded_bone_and_instance_transforms() {
        let positions = [[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let bone = Transform3D::new(Basis::IDENTITY, Vector3::new(0.0, 3.0, 0.0));
        let mut posed = posed_vertices(
            &positions,
            &[0; 12],
            &[1.0, 0.0, 0.0, 0.0].repeat(3),
            &[bone],
        );
        let placement = Transform3D::new(Basis::IDENTITY, Vector3::new(10.0, 0.0, 0.0));
        for point in &mut posed {
            *point = placement * *point;
        }
        assert_eq!(
            surface_hit(
                Vector3::new(10.0, 3.5, 1.0),
                Vector3::new(10.0, 3.5, -1.0),
                &posed,
                Some(&[0, 1, 2])
            ),
            Some((0.5, 0))
        );
        assert!(surface_hit(
            Vector3::new(0.0, 0.5, 1.0),
            Vector3::new(0.0, 0.5, -1.0),
            &posed,
            Some(&[0, 1, 2])
        )
        .is_none());
    }
}
