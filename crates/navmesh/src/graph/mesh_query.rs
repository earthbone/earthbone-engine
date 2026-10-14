// SPDX-License-Identifier: GPL-3.0-or-later

//! Endpoint eligibility follows DetourNavMeshQuery's queryPolygonsInTile filter.
//! Callers supply coordinate-space membership without copying graph geometry.

use super::{NavGraph, NodeId, Path, PathCosts, PathSearch};

/// Supported walking endpoints with an optional connected corridor search.
/// A missing search retains endpoint validity for disconnected-route caching.
pub struct WalkingRequest {
    /// Retained search, or `None` when supported endpoints are disconnected.
    pub search: Option<PathSearch>,
}

impl WalkingRequest {
    /// Heap work in the latest search slice, or zero for disconnected endpoints.
    pub fn last_work(&self) -> usize {
        self.search.as_ref().map_or(0, PathSearch::last_work)
    }
}

impl NavGraph {
    /// Test containment in eligible meshes, including blocked triangles.
    pub fn contains_point_in_meshes(
        &self,
        point: [f32; 3],
        accepts_mesh: impl Fn(u32) -> bool,
    ) -> bool {
        soe_trace::count(soe_trace::Operation::NavmeshLocate, 1);
        self.contains_meshes_impl(point, |_| {}, accepts_mesh)
    }

    /// Locate a point using only eligible meshes, preserving current walkability.
    pub fn locate_in_meshes(
        &self,
        point: [f32; 3],
        accepts_mesh: impl Fn(u32) -> bool,
    ) -> Option<NodeId> {
        soe_trace::count(soe_trace::Operation::NavmeshLocate, 1);
        self.locate_meshes_impl(point, |_| {}, accepts_mesh)
    }

    /// Find the nearest eligible mesh's walkable triangle centroid.
    pub fn nearest_in_meshes(
        &self,
        point: [f32; 3],
        accepts_mesh: impl Fn(u32) -> bool,
    ) -> Option<NodeId> {
        self.nearest_meshes_impl(point, |_| {}, accepts_mesh)
    }

    /// Start a retained search whose endpoints belong to eligible meshes.
    /// Authored adjacency, costs and requested endpoints retain their meaning.
    pub fn start_path_between_in_meshes(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        accepts_mesh: impl Fn(u32) -> bool,
    ) -> Option<PathSearch> {
        self.start_path_between_mesh_sets(from, to, &accepts_mesh, &accepts_mesh)
    }

    /// Scope each endpoint independently, retaining authored links between cells.
    pub fn start_path_between_mesh_sets(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        accepts_start: impl Fn(u32) -> bool,
        accepts_goal: impl Fn(u32) -> bool,
    ) -> Option<PathSearch> {
        let a = self
            .locate_in_meshes(from, &accepts_start)
            .or_else(|| self.nearest_in_meshes(from, &accepts_start))?;
        let b = self
            .locate_in_meshes(to, &accepts_goal)
            .or_else(|| self.nearest_in_meshes(to, &accepts_goal))?;
        self.start_search(a, b, Some((from, to)), PathCosts::default())
    }

    /// Start a walking route with independently eligible endpoint meshes.
    /// Endpoints need walkable support within 64 units of surface Z. Authored
    /// adjacency may connect different endpoint spaces, without a direct fallback.
    ///
    /// # Errors
    /// Returns `None` for unsupported endpoints or a disconnected route.
    ///
    /// # Panics
    /// This function does not panic for valid graph geometry.
    pub fn start_walk_path_between_mesh_sets(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        accepts_start: impl Fn(u32) -> bool,
        accepts_goal: impl Fn(u32) -> bool,
    ) -> Option<PathSearch> {
        self.prepare_walk_path_between_mesh_sets(from, to, accepts_start, accepts_goal)?
            .search
    }

    /// Prepare supported endpoints once, retaining a disconnected result.
    ///
    /// # Errors
    /// Returns `None` when either endpoint lacks walkable support within 64 units.
    ///
    /// # Panics
    /// This function does not panic for valid graph geometry.
    pub fn prepare_walk_path_between_mesh_sets(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        accepts_start: impl Fn(u32) -> bool,
        accepts_goal: impl Fn(u32) -> bool,
    ) -> Option<WalkingRequest> {
        fn endpoint(
            graph: &NavGraph,
            point: [f32; 3],
            accepts_mesh: impl Fn(u32) -> bool,
        ) -> Option<(NodeId, [f32; 3])> {
            soe_trace::count(soe_trace::Operation::NavmeshLocate, 1);
            let (node, z) = graph.locate_surface_meshes_impl(point, |_| {}, accepts_mesh)?;
            if graph.node_blocked(node) {
                return None;
            }
            ((z - point[2]).abs() <= 64.0).then_some((node, [point[0], point[1], z]))
        }
        let (a, from) = endpoint(self, from, accepts_start)?;
        let (b, to) = endpoint(self, to, accepts_goal)?;
        Some(WalkingRequest {
            search: self.start_search(a, b, Some((from, to)), PathCosts::default()),
        })
    }

    /// Query a path whose endpoint triangles belong to eligible meshes.
    pub fn find_path_between_in_meshes(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        accepts_mesh: impl Fn(u32) -> bool,
    ) -> Option<Path> {
        let mut search = self.start_path_between_in_meshes(from, to, accepts_mesh)?;
        match search.advance(self, usize::MAX) {
            std::task::Poll::Ready(path) => path,
            std::task::Poll::Pending => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> NavGraph {
        let mut graph = NavGraph::new();
        for height in [0.0, 10.0] {
            graph.add_mesh_with_adjacency(
                vec![
                    [0.0, 0.0, height],
                    [100.0, 0.0, height],
                    [0.0, 100.0, height],
                ],
                &[[0, 1, 2]],
            );
        }
        graph
    }

    #[test]
    fn endpoint_mesh_filter_excludes_a_closer_overlapping_room() {
        let graph = graph();
        let point = [20.0, 20.0, 10.0];
        assert_eq!(graph.locate(point), Some(NodeId::new(1, 0)));
        assert_eq!(
            graph.locate_in_meshes(point, |mesh| mesh == 0),
            Some(NodeId::new(0, 0))
        );
        assert_eq!(
            graph.nearest_in_meshes([200.0, 200.0, 10.0], |mesh| mesh == 0),
            Some(NodeId::new(0, 0))
        );
        let path = graph
            .find_path_between_in_meshes(point, [60.0, 20.0, 10.0], |mesh| mesh == 0)
            .unwrap();
        assert!(path.nodes.iter().all(|node| node.mesh == 0));
    }

    #[test]
    fn endpoint_mesh_filter_rejects_missing_and_blocked_eligible_space() {
        let mut graph = graph();
        let point = [20.0, 20.0, 10.0];
        assert!(graph
            .start_path_between_in_meshes(point, [60.0, 20.0, 10.0], |_| false)
            .is_none());
        graph.set_blocked(NodeId::new(0, 0), true);
        assert!(graph.locate_in_meshes(point, |mesh| mesh == 0).is_none());
        assert!(graph.nearest_in_meshes(point, |mesh| mesh == 0).is_none());
    }

    #[test]
    fn walking_mesh_masks_project_supported_endpoints_and_reject_unsupported_approaches() {
        let mut graph = graph();
        let from = [20.0, 20.0, 10.0];
        let to = [60.0, 20.0, 10.0];
        let mut search = graph
            .start_walk_path_between_mesh_sets(from, to, |mesh| mesh == 0, |mesh| mesh == 0)
            .unwrap();
        let std::task::Poll::Ready(Some(path)) = search.advance(&graph, usize::MAX) else {
            panic!("supported route must complete");
        };
        assert_eq!(path.points, [[20.0, 20.0, 0.0], [60.0, 20.0, 0.0]]);
        for to in [[-20.0, 20.0, 0.0], [60.0, 20.0, 65.0]] {
            assert!(graph
                .start_walk_path_between_mesh_sets(from, to, |mesh| mesh == 0, |mesh| mesh == 0)
                .is_none());
        }
        graph.set_blocked(NodeId::new(0, 0), true);
        assert!(graph
            .start_walk_path_between_mesh_sets(from, to, |mesh| mesh == 0, |mesh| mesh == 0)
            .is_none());
    }

    #[test]
    fn walking_mesh_masks_preserve_authored_links_between_endpoint_spaces() {
        let mut graph = graph();
        let from = [20.0, 20.0, 0.0];
        let to = [60.0, 20.0, 10.0];
        assert!(graph
            .start_walk_path_between_mesh_sets(from, to, |mesh| mesh == 0, |mesh| mesh == 1)
            .is_none());
        graph.link(NodeId::new(0, 0), 0, NodeId::new(1, 0));
        let mut search = graph
            .start_walk_path_between_mesh_sets(from, to, |mesh| mesh == 0, |mesh| mesh == 1)
            .unwrap();
        let std::task::Poll::Ready(Some(path)) = search.advance(&graph, usize::MAX) else {
            panic!("authored link must remain traversable");
        };
        assert_eq!(path.nodes, [NodeId::new(0, 0), NodeId::new(1, 0)]);
    }
}
