// SPDX-License-Identifier: GPL-3.0-or-later

//! Record-only full-model demand and terrain sight lines. OBND follows xEdit's
//! `wbObjectBounds` in `wbDefinitionsCommon.pas` and UESP `OBND_Field`.
//! Mudcrab PR 210 supplies nearby demand before loading, PR 211 bounds admitted
//! jobs, and PR 225 keeps uncertain visibility drawn.
//! The host retains Godot's whole-instance AABB occlusion test.

use soe_esm::GlobalFormId;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// Selection works in Skyrim XYZ units, independent of camera heading.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: GlobalFormId,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Candidate {
    /// Transform all eight OBND corners, including reference scale and rotation.
    pub fn from_bounds(
        id: GlobalFormId,
        bounds: [i16; 6],
        position: [f32; 3],
        rotation: [f32; 3],
        scale: f32,
    ) -> Option<Self> {
        if !position.iter().chain(&rotation).all(|v| v.is_finite()) || !scale.is_finite() {
            return None;
        }
        let matrix = crate::units::skyrim_euler_matrix(rotation);
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for corner in 0..8 {
            let local: [f32; 3] = std::array::from_fn(|axis| {
                f32::from(bounds[axis + if corner & (1 << axis) == 0 { 0 } else { 3 }]) * scale
            });
            for axis in 0..3 {
                let p = position[axis] + (0..3).map(|i| matrix[axis][i] * local[i]).sum::<f32>();
                if !p.is_finite() {
                    return None;
                }
                min[axis] = min[axis].min(p);
                max[axis] = max[axis].max(p);
            }
        }
        Some(Self { id, min, max })
    }

    pub fn distance(&self, eye: [f32; 3]) -> f32 {
        (0..3)
            .map(|i| (eye[i] - eye[i].clamp(self.min[i], self.max[i])).powi(2))
            .sum::<f32>()
            .sqrt()
            .max(1.0)
    }

    /// Diameter projected onto a 720 pixel, 75 degree vertical field of view.
    pub fn pixels(&self, eye: [f32; 3]) -> f32 {
        let diameter = (0..3)
            .map(|i| (self.max[i] - self.min[i]).powi(2))
            .sum::<f32>()
            .sqrt();
        diameter * 720.0 / (2.0 * (75.0_f32.to_radians() / 2.0).tan() * self.distance(eye))
    }

    /// Any clear corner or the top centre keeps a partly exposed landmark.
    pub fn visible(&self, eye: [f32; 3], height: &impl Fn(f32, f32) -> Option<f32>) -> bool {
        (0..9).any(|corner| {
            let point = if corner == 8 {
                [
                    self.min[0].midpoint(self.max[0]),
                    self.min[1].midpoint(self.max[1]),
                    self.max[2],
                ]
            } else {
                std::array::from_fn(|axis| {
                    if corner & (1 << axis) == 0 {
                        self.min[axis]
                    } else {
                        self.max[axis]
                    }
                })
            };
            line_clear(eye, point, height)
        })
    }
}

fn line_clear(eye: [f32; 3], point: [f32; 3], height: &impl Fn(f32, f32) -> Option<f32>) -> bool {
    let distance = ((point[0] - eye[0]).powi(2) + (point[1] - eye[1]).powi(2)).sqrt();
    let steps = (distance / 128.0).ceil().clamp(1.0, 8192.0) as usize;
    for step in 1..steps {
        let t = step as f32 / steps as f32;
        let p: [f32; 3] = std::array::from_fn(|i| eye[i] + (point[i] - eye[i]) * t);
        // SAFETY: Lower the resident surface by two metres. Missing coverage
        // never supplies a blocker, and a grazing sight line stays visible.
        if height(p[0], p[1]).is_some_and(|h| h - 140.0 > p[2]) {
            return false;
        }
    }
    true
}

#[derive(Default)]
struct Residency {
    hidden_checks: u8,
    hidden_frames: u32,
}

/// Movement-triggered selection with wider release reach and hidden debounce.
#[derive(Default)]
pub struct Selector {
    last_eye: Option<[f32; 3]>,
    revision: u64,
    resident: BTreeMap<GlobalFormId, Residency>,
    pending: VecDeque<GlobalFormId>,
}

impl Selector {
    /// Cheap movement, terrain, pending admission and deferred release test.
    pub fn needs_evaluation(&self, eye: [f32; 3], revision: u64) -> bool {
        self.revision != revision
            || !self.pending.is_empty()
            || self
                .resident
                .values()
                .any(|state| state.hidden_checks >= 3 && state.hidden_frames >= 29)
            || self.last_eye.is_none_or(|last| {
                (0..3).map(|i| (last[i] - eye[i]).powi(2)).sum::<f32>() >= 512.0_f32.powi(2)
            })
    }

    /// Evaluate after 512 units of movement or a terrain revision. Hidden
    /// references survive three observations and thirty update frames.
    /// Admission is nearest first, at most eight new references per update.
    pub fn update(
        &mut self,
        candidates: &[Candidate],
        eye: [f32; 3],
        revision: u64,
        height: impl Fn(f32, f32) -> Option<f32>,
    ) -> Vec<GlobalFormId> {
        for state in self.resident.values_mut().filter(|s| s.hidden_checks > 0) {
            state.hidden_frames = state.hidden_frames.saturating_add(1);
        }
        let moved = self.last_eye.is_none_or(|last| {
            (0..3).map(|i| (last[i] - eye[i]).powi(2)).sum::<f32>() >= 512.0_f32.powi(2)
        });
        if moved || self.revision != revision {
            self.last_eye = Some(eye);
            self.revision = revision;
            let mut additions = Vec::new();
            for candidate in candidates {
                let existing = self.resident.get_mut(&candidate.id);
                let pixels = candidate.pixels(eye);
                let visible = pixels >= if existing.is_some() { 1.5 } else { 2.0 }
                    && candidate.visible(eye, &height);
                if let Some(state) = existing {
                    if visible {
                        *state = Residency::default();
                    } else {
                        if state.hidden_checks == 0 {
                            state.hidden_frames = 1;
                        }
                        state.hidden_checks = state.hidden_checks.saturating_add(1);
                    }
                } else if visible {
                    additions.push((candidate.distance(eye), candidate.id));
                }
            }
            self.resident
                .retain(|id, _| candidates.iter().any(|c| c.id == *id));
            additions.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            self.pending = additions.into_iter().map(|(_, id)| id).collect();
        }
        self.resident
            .retain(|_, state| !(state.hidden_checks >= 3 && state.hidden_frames >= 30));
        self.pending
            .retain(|id| candidates.iter().any(|candidate| candidate.id == *id));
        for id in self.pending.drain(..self.pending.len().min(8)) {
            self.resident.insert(id, Residency::default());
        }
        self.resident.keys().copied().collect()
    }
}

/// Spatial height samples derived from resident BTR triangles, without reads.
#[derive(Default)]
pub struct TerrainHeights {
    bins: HashMap<(i32, i32), Vec<usize>>,
    triangles: Vec<[[f32; 3]; 3]>,
    near: HashMap<crate::streaming::CellCoord, Vec<f32>>,
    near_hidden: HashMap<crate::streaming::CellCoord, u32>,
}

impl TerrainHeights {
    /// Approximate retained height geometry and bin allocation bytes.
    pub fn memory_bytes(&self) -> usize {
        self.triangles.capacity() * std::mem::size_of::<[[f32; 3]; 3]>()
            + self.bins.capacity() * std::mem::size_of::<((i32, i32), Vec<usize>)>()
            + self
                .bins
                .values()
                .map(|indices| indices.capacity() * std::mem::size_of::<usize>())
                .sum::<usize>()
            + self.near_hidden.capacity()
                * std::mem::size_of::<(crate::streaming::CellCoord, u32)>()
            + self
                .near
                .values()
                .map(|heights| heights.capacity() * std::mem::size_of::<f32>())
                .sum::<usize>()
    }

    /// Resident LAND replaces BTR under the same join mask. Heights are
    /// Godot metres, in the rendered south-to-north 33 by 33 layout.
    pub fn set_near(&mut self, near: &HashMap<crate::streaming::CellCoord, Vec<f32>>) {
        self.near.clone_from(near);
        self.near_hidden.clear();
    }

    /// Hidden CELL quadrants retain coverage but supply no visibility blocker.
    pub fn set_near_hidden(&mut self, hidden: HashMap<crate::streaming::CellCoord, u32>) {
        self.near_hidden = hidden;
    }

    /// Index upward terrain in Skyrim units. Water is excluded by the caller.
    pub fn insert(&mut self, positions: &[[f32; 3]], indices: &[i32]) {
        for indices in indices.as_chunks::<3>().0 {
            let Some(points) = indices
                .iter()
                .map(|&i| usize::try_from(i).ok().and_then(|i| positions.get(i)))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let triangle: [[f32; 3]; 3] = std::array::from_fn(|i| {
                [
                    points[i][0] * 70.0,
                    -points[i][2] * 70.0,
                    points[i][1] * 70.0,
                ]
            });
            let min: [f32; 2] = std::array::from_fn(|i| {
                triangle.iter().map(|p| p[i]).fold(f32::INFINITY, f32::min)
            });
            let max: [f32; 2] = std::array::from_fn(|i| {
                triangle
                    .iter()
                    .map(|p| p[i])
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            if triangle.iter().flatten().any(|v| !v.is_finite()) {
                continue;
            }
            let slot = self.triangles.len();
            self.triangles.push(triangle);
            for y in (min[1] / 512.0).floor() as i32..=(max[1] / 512.0).floor() as i32 {
                for x in (min[0] / 512.0).floor() as i32..=(max[0] / 512.0).floor() as i32 {
                    self.bins.entry((x, y)).or_default().push(slot);
                }
            }
        }
    }

    pub fn sample(&self, x: f32, y: f32) -> Option<f32> {
        let coord = crate::streaming::CellCoord::new(
            (x / 4096.0).floor() as i32,
            (y / 4096.0).floor() as i32,
        );
        if let Some(heights) = self.near.get(&coord) {
            if heights.len() != 33 * 33 {
                return None;
            }
            let col = (x / 128.0 - coord.x as f32 * 32.0).clamp(0.0, 32.0);
            let row = (y / 128.0 - coord.y as f32 * 32.0).clamp(0.0, 32.0);
            let c = (col.floor() as usize).min(31);
            let r = (row.floor() as usize).min(31);
            let quadrant = c / 16 + 2 * (r / 16);
            if self.near_hidden.get(&coord).copied().unwrap_or(0) & (1 << quadrant) != 0 {
                return None;
            }
            let u = col - c as f32;
            let v = row - r as f32;
            let sw = heights[r * 33 + c];
            let se = heights[r * 33 + c + 1];
            let nw = heights[(r + 1) * 33 + c];
            let ne = heights[(r + 1) * 33 + c + 1];
            // CONTEXT: exterior.rs renders the SW to NE diagonal.
            let h = if u >= v {
                sw * (1.0 - u) + se * (u - v) + ne * v
            } else {
                sw * (1.0 - v) + nw * (v - u) + ne * u
            };
            return h.is_finite().then_some(h * 70.0);
        }

        self.bins
            .get(&((x / 512.0).floor() as i32, (y / 512.0).floor() as i32))?
            .iter()
            .filter_map(|&slot| {
                let [a, b, c] = self.triangles[slot];
                let denominator = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if denominator.abs() < 0.001 {
                    return None;
                }
                let u = ((b[1] - c[1]) * (x - c[0]) + (c[0] - b[0]) * (y - c[1])) / denominator;
                let v = ((c[1] - a[1]) * (x - c[0]) + (a[0] - c[0]) * (y - c[1])) / denominator;
                (u >= 0.0 && v >= 0.0 && u + v <= 1.0)
                    .then_some(u * a[2] + v * b[2] + (1.0 - u - v) * c[2])
            })
            .reduce(f32::min)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hidden_land_heights_do_not_occlude_or_fall_back_to_distant_terrain() {
        use super::TerrainHeights;
        use crate::streaming::CellCoord;
        use std::collections::HashMap;

        let coord = CellCoord::new(-2, 3);
        let mut heights = TerrainHeights::default();
        heights.insert(
            &[
                [-200.0, 200.0, -100.0],
                [0.0, 200.0, -100.0],
                [-100.0, 200.0, -300.0],
            ],
            &[0, 1, 2],
        );
        for hidden in 0..16 {
            heights.set_near(&HashMap::from([(coord, vec![100.0; 33 * 33])]));
            heights.set_near_hidden(HashMap::from([(coord, hidden)]));
            for (quadrant, x, y) in [
                (0, -7168.0, 13312.0),
                (1, -5120.0, 13312.0),
                (2, -7168.0, 15360.0),
                (3, -5120.0, 15360.0),
            ] {
                assert_eq!(
                    heights.sample(x, y),
                    (hidden & (1 << quadrant) == 0).then_some(7000.0),
                    "quadrant {quadrant}, mask {hidden:x}"
                );
            }
        }
    }

    use super::*;
    fn candidate(top: i16) -> Candidate {
        Candidate::from_bounds(
            GlobalFormId { file: 0, object: 1 },
            [-100, -100, 0, 100, 100, top],
            [4000.0, 0.0, 0.0],
            [0.0; 3],
            1.0,
        )
        .unwrap()
    }
    fn ridge(x: f32, _: f32) -> Option<f32> {
        if !x.is_finite() {
            return None;
        }
        Some(if (1500.0..2500.0).contains(&x) {
            800.0
        } else {
            0.0
        })
    }
    #[test]
    fn reference_behind_ridge_is_not_loaded() {
        assert!(Selector::default()
            .update(&[candidate(200)], [0.0, 0.0, 100.0], 0, ridge)
            .is_empty());
    }
    #[test]
    fn top_over_ridge_is_loaded() {
        assert_eq!(
            Selector::default()
                .update(&[candidate(2000)], [0.0, 0.0, 100.0], 0, ridge)
                .len(),
            1
        );
    }
    #[test]
    fn missing_terrain_is_visible() {
        assert!(candidate(200).visible([0.0, 0.0, 100.0], &|_, _| None));
    }
    #[test]
    fn hidden_reference_survives_until_three_observations_and_thirty_frames() {
        let mut selector = Selector::default();
        let candidates = [candidate(2000)];
        assert_eq!(
            selector
                .update(&candidates, [0.0, 0.0, 100.0], 0, ridge)
                .len(),
            1
        );
        for frame in 1..=29 {
            assert_eq!(
                selector
                    .update(&candidates, [0.0, 0.0, 100.0], frame, |_, _| Some(10000.0))
                    .len(),
                1
            );
        }
        assert!(selector
            .update(&candidates, [0.0, 0.0, 100.0], 31, |_, _| Some(10000.0))
            .is_empty());
    }
    #[test]
    fn hidden_release_finishes_at_rest_without_resampling_terrain() {
        let candidates = [candidate(2000)];
        let eye = [0.0, 0.0, 100.0];
        let mut selector = Selector::default();
        selector.update(&candidates, eye, 0, ridge);
        for revision in 1..=3 {
            assert_eq!(
                selector
                    .update(&candidates, eye, revision, |_, _| Some(10000.0))
                    .len(),
                1
            );
        }
        for _frame in 4..=29 {
            assert!(!selector.needs_evaluation(eye, 3));
            assert_eq!(
                selector
                    .update(&[], eye, 3, |_, _| panic!("idle terrain query"))
                    .len(),
                1
            );
        }
        assert!(selector.needs_evaluation(eye, 3));
        assert!(selector
            .update(&candidates, eye, 3, |_, _| panic!("release terrain query"))
            .is_empty());
        assert!(!selector.needs_evaluation(eye, 3));
    }

    #[test]
    fn scale_and_rotation_transform_every_corner() {
        let candidate = Candidate::from_bounds(
            GlobalFormId { file: 0, object: 1 },
            [-10, -20, -30, 10, 20, 30],
            [100.0; 3],
            [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            2.0,
        )
        .unwrap();
        assert!((candidate.min[0] - 60.0).abs() < 0.001);
        assert!((candidate.max[1] - 120.0).abs() < 0.001);
        assert_eq!(candidate.max[2], 160.0);
    }

    #[test]
    fn zero_scale_creates_no_distant_demand() {
        let candidate = Candidate::from_bounds(
            GlobalFormId { file: 0, object: 1 },
            [-100, -100, 0, 100, 100, 2000],
            [4000.0, 0.0, 0.0],
            [0.0; 3],
            0.0,
        )
        .unwrap();
        assert_eq!(candidate.pixels([0.0; 3]), 0.0);
        assert!(Selector::default()
            .update(&[candidate], [0.0; 3], 0, |_, _| None)
            .is_empty());
    }
    #[test]
    fn release_reach_exceeds_admission_reach() {
        let candidates = [candidate(200)];
        let mut selector = Selector::default();
        let reach = candidates[0].pixels([0.0; 3]) * 4000.0 / 2.0;
        let eye = [4000.0 - reach * 0.95, 0.0, 0.0];
        assert_eq!(selector.update(&candidates, eye, 0, |_, _| None).len(), 1);
        let farther = [4000.0 - reach * 1.2, 0.0, 0.0];
        assert_eq!(
            selector.update(&candidates, farther, 0, |_, _| None).len(),
            1
        );
        assert!(Selector::default()
            .update(&candidates, farther, 0, |_, _| None)
            .is_empty());
    }
    #[test]
    fn idle_frames_do_not_resample_terrain() {
        let mut selector = Selector::default();
        let candidates = [candidate(200)];
        selector.update(&candidates, [0.0; 3], 0, |_, _| None);
        let sampled = std::cell::Cell::new(false);
        selector.update(&candidates, [100.0, 0.0, 0.0], 0, |_, _| {
            sampled.set(true);
            None
        });
        assert!(!sampled.get());
    }
    #[test]
    fn admission_is_bounded_and_nearest_first() {
        let candidates: Vec<_> = (1..=16)
            .map(|object| {
                Candidate::from_bounds(
                    GlobalFormId { file: 0, object },
                    [-100, -100, 0, 100, 100, 500],
                    [object as f32 * 1000.0, 0.0, 0.0],
                    [0.0; 3],
                    1.0,
                )
                .unwrap()
            })
            .collect();
        let selected = Selector::default().update(&candidates, [0.0; 3], 0, |_, _| None);
        assert_eq!(
            selected.iter().map(|id| id.object).collect::<Vec<_>>(),
            (1..=8).collect::<Vec<_>>()
        );
    }

    #[test]
    fn pending_admission_does_not_resample_terrain() {
        let candidates: Vec<_> = (1..=16)
            .map(|object| Candidate {
                id: GlobalFormId { file: 0, object },
                ..candidate(2000)
            })
            .collect();
        let eye = [0.0, 0.0, 100.0];
        let mut selector = Selector::default();
        assert_eq!(selector.update(&candidates, eye, 0, ridge).len(), 8);
        assert!(selector.needs_evaluation(eye, 0));
        assert_eq!(
            selector
                .update(&candidates, eye, 0, |_, _| panic!("pending terrain query"))
                .len(),
            16
        );
        assert!(!selector.needs_evaluation(eye, 0));
    }

    #[test]
    fn terrain_change_cancels_pending_hidden_demand() {
        let candidates: Vec<_> = (1..=16)
            .map(|object| Candidate {
                id: GlobalFormId { file: 0, object },
                ..candidate(2000)
            })
            .collect();
        let eye = [0.0, 0.0, 100.0];
        let mut selector = Selector::default();
        assert_eq!(selector.update(&candidates, eye, 0, ridge).len(), 8);
        assert_eq!(
            selector
                .update(&candidates, eye, 1, |_, _| Some(10000.0))
                .len(),
            8
        );
        assert!(!selector.needs_evaluation(eye, 1));
    }
    #[test]
    fn loaded_land_replaces_the_hidden_btr_height() {
        let mut heights = TerrainHeights::default();
        heights.insert(
            &[[0.0, 100.0, 0.0], [10.0, 100.0, 0.0], [0.0, 100.0, -10.0]],
            &[0, 1, 2],
        );
        assert_eq!(heights.sample(350.0, 100.0), Some(7000.0));
        heights.set_near(&HashMap::from([(
            crate::streaming::CellCoord::new(0, 0),
            vec![0.0; 33 * 33],
        )]));
        assert_eq!(heights.sample(350.0, 100.0), Some(0.0));
    }
    #[test]
    fn height_samples_follow_triangles_and_leave_holes_open() {
        let mut heights = TerrainHeights::default();
        heights.insert(
            &[[0.0, 1.0, 0.0], [10.0, 2.0, 0.0], [0.0, 1.0, -10.0]],
            &[0, 1, 2],
        );
        assert!((heights.sample(350.0, 100.0).unwrap() - 105.0).abs() < 0.001);
        assert_eq!(heights.sample(690.0, 690.0), None);
    }
}
