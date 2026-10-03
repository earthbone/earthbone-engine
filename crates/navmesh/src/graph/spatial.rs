// SPDX-License-Identifier: GPL-3.0-or-later

//! Median-split mesh and triangle bounds, following Detour's bounding volume tree in
//! DetourNavMeshBuilder.cpp and DetourNavMeshQuery.cpp. Float bounds retain
//! this graph's centroid and XY-height query semantics without tile quantization.

use super::Mesh;

#[derive(Debug, Clone)]
struct Node {
    min: [f32; 3],
    max: [f32; 3],
    end: usize,
    mesh: Option<usize>,
}

impl Node {
    fn distance(&self, point: [f32; 3]) -> f32 {
        let mut squared = 0.0;
        for (axis, p) in point.iter().enumerate() {
            let d = (self.min[axis] - p).max(0.0).max(p - self.max[axis]);
            squared += d * d;
        }
        squared.sqrt()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct SpatialIndex {
    nodes: Vec<Node>,
}

impl SpatialIndex {
    pub(super) fn triangles(mesh: &Mesh) -> Self {
        let mut items: Vec<_> = mesh
            .triangles
            .iter()
            .enumerate()
            .map(|(index, triangle)| {
                let points = triangle.vertices.map(|v| mesh.vertices[v as usize]);
                let mut min = [f32::INFINITY; 3];
                let mut max = [f32::NEG_INFINITY; 3];
                for point in points {
                    for axis in 0..3 {
                        min[axis] = min[axis].min(point[axis]);
                        max[axis] = max[axis].max(point[axis]);
                    }
                }
                // CONTEXT: height_at accepts barycentric weights down to -1e-4.
                // Two negative weights can extend a point beyond a vertex bound.
                for axis in 0..3 {
                    let margin = (max[axis] - min[axis]) * 2e-4
                        + min[axis].abs().max(max[axis].abs()).max(1.0) * f32::EPSILON * 4.0;
                    min[axis] -= margin;
                    max[axis] += margin;
                }
                (index, min, max)
            })
            .collect();
        let mut index = Self::default();
        if !items.is_empty() {
            index.split(&mut items);
        }
        index
    }

    pub(super) fn build(meshes: &[Mesh]) -> Self {
        let mut items: Vec<_> = meshes
            .iter()
            .enumerate()
            .filter(|(_, mesh)| !mesh.triangles.is_empty())
            .map(|(index, mesh)| {
                let (mut min, mut max) = mesh.bounds();
                // Rounded centroids can lie just outside their vertex bounds.
                for triangle in 0..mesh.triangles.len() {
                    let centroid = mesh.centroid(triangle as u32);
                    for axis in 0..3 {
                        min[axis] = min[axis].min(centroid[axis]);
                        max[axis] = max[axis].max(centroid[axis]);
                    }
                }
                (index, min, max)
            })
            .collect();
        let mut index = Self::default();
        if !items.is_empty() {
            index.split(&mut items);
        }
        index
    }

    fn split(&mut self, items: &mut [(usize, [f32; 3], [f32; 3])]) {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for (_, low, high) in items.iter() {
            for axis in 0..3 {
                min[axis] = min[axis].min(low[axis]);
                max[axis] = max[axis].max(high[axis]);
            }
        }
        let at = self.nodes.len();
        self.nodes.push(Node {
            min,
            max,
            end: at + 1,
            mesh: None,
        });
        if items.len() == 1 {
            self.nodes[at].mesh = Some(items[0].0);
            return;
        }
        let mut axis = 0;
        for next in 1..3 {
            if max[next] - min[next] > max[axis] - min[axis] {
                axis = next;
            }
        }
        let middle = items.len() / 2;
        items.select_nth_unstable_by(middle, |a, b| {
            a.1[axis]
                .midpoint(a.2[axis])
                .total_cmp(&b.1[axis].midpoint(b.2[axis]))
                .then_with(|| a.0.cmp(&b.0))
        });
        let (left, right) = items.split_at_mut(middle);
        self.split(left);
        self.split(right);
        self.nodes[at].end = self.nodes.len();
    }

    pub(super) fn visit_xy(&self, point: [f32; 3], mut visit: impl FnMut(usize)) {
        let mut at = 0;
        while let Some(node) = self.nodes.get(at) {
            if point[0] < node.min[0]
                || point[0] > node.max[0]
                || point[1] < node.min[1]
                || point[1] > node.max[1]
            {
                at = node.end;
            } else {
                if let Some(mesh) = node.mesh {
                    visit(mesh);
                }
                at += 1;
            }
        }
    }

    pub(super) fn visit_nearest(&self, point: [f32; 3], mut visit: impl FnMut(usize, &mut f32)) {
        if !self.nodes.is_empty() {
            let mut radius = f32::INFINITY;
            self.nearest(0, point, &mut radius, &mut visit);
        }
    }

    fn nearest(
        &self,
        at: usize,
        point: [f32; 3],
        radius: &mut f32,
        visit: &mut impl FnMut(usize, &mut f32),
    ) {
        let node = &self.nodes[at];
        if node.distance(point) > *radius {
            return;
        }
        if let Some(mesh) = node.mesh {
            visit(mesh, radius);
        } else {
            let left = at + 1;
            let right = self.nodes[left].end;
            let (first, second) =
                if self.nodes[left].distance(point) <= self.nodes[right].distance(point) {
                    (left, right)
                } else {
                    (right, left)
                };
            self.nearest(first, point, radius, visit);
            self.nearest(second, point, radius, visit);
        }
    }
}
