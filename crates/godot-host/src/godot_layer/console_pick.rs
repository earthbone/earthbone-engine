// SPDX-License-Identifier: GPL-3.0-or-later

//! Console picking follows uploaded batches and MultiMesh placements.

use super::{BatchTable, CellStreamerNode, InstanceMesh, Vector3};
use crate::render_pick::{nearer, surface_hit, Pick};

impl CellStreamerNode {
    pub(crate) fn console_geometry_pick(&self, from: Vector3, to: Vector3) -> Option<Pick> {
        let mut best = None;
        for instance in &self.instances {
            if !self.console_visibility[instance.render_kind] {
                continue;
            }
            let visible = |reference: &soe_esm::GlobalFormId| {
                self.ref_cache
                    .get(reference)
                    .is_none_or(|(visible, _, _)| *visible)
            };
            if let Some(animation) = &instance.animation {
                let BatchTable::Triangles(members) = &instance.table else {
                    continue;
                };
                let Some(member) = members.first().filter(|member| visible(&member.reference))
                else {
                    continue;
                };
                let Some(placement) = animation.last_transforms.first() else {
                    continue;
                };
                if placement.basis.determinant().abs() < 1e-10 {
                    continue;
                }
                let inverse = placement.affine_inverse();
                let vertices = crate::render_pick::posed_vertices(
                    &instance.arrays.positions,
                    &instance.arrays.bones,
                    &instance.arrays.weights,
                    &animation.last_transforms[1..],
                );
                if let Some((fraction, _)) = surface_hit(
                    inverse * from,
                    inverse * to,
                    &vertices,
                    Some(&instance.arrays.indices),
                ) {
                    nearer(&mut best, (fraction, member.reference));
                }
                continue;
            }
            match (&instance.mesh, &instance.table) {
                (InstanceMesh::Array(mesh), BatchTable::Triangles(members)) => {
                    if !mesh.get_aabb().intersects_segment(from, to) {
                        continue;
                    }
                    let faces = mesh.get_faces();
                    for member in members.iter().filter(|member| visible(&member.reference)) {
                        let start = member.member.first_triangle * 3;
                        let end = start + member.member.triangle_count * 3;
                        let Some(faces) = faces.as_slice().get(start..end) else {
                            continue;
                        };
                        if let Some((fraction, _)) = surface_hit(from, to, faces, None) {
                            nearer(&mut best, (fraction, member.reference));
                        }
                    }
                }
                (InstanceMesh::Multi(multi), BatchTable::Instances(references)) => {
                    if !multi.get_aabb().intersects_segment(from, to) {
                        continue;
                    }
                    let Some(mesh) = multi.get_mesh() else {
                        continue;
                    };
                    let faces = mesh.get_faces();
                    for (index, &reference) in references
                        .iter()
                        .enumerate()
                        .filter(|(_, reference)| visible(reference))
                    {
                        let transform = multi.get_instance_transform(index as i32);
                        if transform.basis.determinant().abs() < 1e-10 {
                            continue;
                        }
                        let inverse = transform.affine_inverse();
                        let start = inverse * from;
                        let end = inverse * to;
                        if !mesh.get_aabb().intersects_segment(start, end) {
                            continue;
                        }
                        if let Some((fraction, _)) = surface_hit(start, end, faces.as_slice(), None)
                        {
                            nearer(&mut best, (fraction, reference));
                        }
                    }
                }
                _ => {}
            }
        }
        for (&reference, door) in &self.doors {
            if !door.node.is_visible() {
                continue;
            }
            for surface in &door.surfaces {
                if surface.submitted.basis.determinant().abs() < 1e-10 {
                    continue;
                }
                let inverse = surface.submitted.affine_inverse();
                let start = inverse * from;
                let end = inverse * to;
                if !surface.mesh.get_aabb().intersects_segment(start, end) {
                    continue;
                }
                let faces = surface.mesh.get_faces();
                if let Some((fraction, _)) = surface_hit(start, end, faces.as_slice(), None) {
                    nearer(&mut best, (fraction, reference));
                }
            }
        }
        best
    }
}
