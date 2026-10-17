// SPDX-License-Identifier: GPL-3.0-or-later

//! NIF object poses and private material state, using nifly controller fields
//! and the existing host skin transform convention from skinned.rs.

use std::sync::Arc;

use soe_nif::object_animation::{ObjectAnimation, ObjectPose, PropertyChannel};
use soe_nif::{mesh::Skin, Transform};

use crate::mesh::MaterialDesc;
use crate::skinned::Affine;

/// Transient door playback derived from World's open state. Sequence tracks
/// retain their child-node bindings and never alter the reference placement.
#[derive(Debug)]
pub struct DoorAnimation {
    pub asset: Arc<ObjectAnimation>,
    fraction: f32,
    opened: bool,
    sequence: &'static str,
}

impl DoorAnimation {
    /// Retain an authored door sequence even if its opposite is absent.
    /// A missing sequence holds the pose instead of rotating the reference.
    pub fn new(asset: Arc<ObjectAnimation>, opened: bool) -> Option<Self> {
        asset.has_sequences().then_some(Self {
            asset,
            fraction: if opened { 1.0 } else { 0.0 },
            opened,
            sequence: "Open",
        })
    }

    /// Select Open or Close on a state change, preserving swing progress.
    pub fn set_opened(&mut self, opened: bool) {
        if self.opened != opened {
            self.opened = opened;
            self.sequence = if opened { "Open" } else { "Close" };
        }
    }

    /// Advance at the authored sequence rate and hold the final pose.
    pub fn advance(&mut self, delta: f32) -> bool {
        if !delta.is_finite() || delta <= 0.0 {
            return false;
        }
        let Some(duration) = self.asset.sequence_duration(self.sequence) else {
            return false;
        };
        let step = if duration > 0.0 {
            delta / duration
        } else {
            1.0
        };
        let next = if self.opened {
            (self.fraction + step).min(1.0)
        } else {
            (self.fraction - step).max(0.0)
        };
        let changed = self.fraction != next;
        self.fraction = next;
        changed
    }

    /// Current sequence name and elapsed seconds for diagnostics and sampling.
    pub fn playback(&self) -> (&str, f32) {
        let duration = self.asset.sequence_duration(self.sequence).unwrap_or(0.0);
        let progress = if self.sequence == "Open" {
            self.fraction
        } else {
            1.0 - self.fraction
        };
        (self.sequence, progress * duration)
    }

    /// Sample the leaf hierarchy, holding the authored rest pose for a missing track.
    pub fn pose(&self) -> ObjectPose {
        let (name, elapsed) = self.playback();
        if self.asset.sequence_duration(name).is_some() {
            self.asset.sample(elapsed, Some(name))
        } else {
            self.asset.rest_pose()
        }
    }

    /// Fraction between the closed and open endpoints.
    pub fn fraction(&self) -> f32 {
        self.fraction
    }
}

/// Transient playback clock shared by the pure sampler and RID update path.
#[cfg(any(feature = "godot", test))]
#[derive(Debug, Default)]
pub(crate) struct ObjectAnimationClock {
    elapsed: f64,
}

#[cfg(any(feature = "godot", test))]
impl ObjectAnimationClock {
    /// The elapsed seconds used by both CPU tracks and generated shader tracks.
    pub(crate) fn elapsed_seconds(&self) -> f32 {
        self.elapsed as f32
    }

    pub(crate) fn reset(&mut self) {
        self.elapsed = 0.0;
    }

    pub(crate) fn update(
        &mut self,
        asset: &ObjectAnimation,
        delta: f32,
        sequence: Option<&str>,
    ) -> ObjectPose {
        if delta.is_finite() && delta > 0.0 {
            self.elapsed = (self.elapsed + f64::from(delta)).min(f64::from(f32::MAX));
        }
        asset.sample(self.elapsed_seconds(), sequence)
    }
}

#[cfg(test)]
mod clock_tests {
    use super::{DoorAnimation, ObjectAnimation, ObjectAnimationClock};
    use crate::data_source::fixture::{Buf, NifWriter, IDENTITY};
    use std::sync::Arc;

    fn door_asset(close: bool, manager_flags: u16) -> Arc<ObjectAnimation> {
        let mut writer = NifWriter::new(100);
        let root = writer.reserve("NiNode");
        let hinge = writer.reserve("NiNode");
        let manager = writer.reserve("NiControllerManager");
        let mut sequences = Vec::new();
        for (name, from, to, duration, frequency) in
            [("Open", 0.0, 1.0, 2.0, 2.0), ("Close", 1.0, 0.0, 3.0, 1.0)]
        {
            if name == "Close" && !close {
                continue;
            }
            let rotation = |angle: f32| [(angle * 0.5).cos(), 0.0, 0.0, (angle * 0.5).sin()];
            let data = writer.transform_data_linear(
                &[(5.0, rotation(from)), (5.0 + duration, rotation(to))],
                &[],
                &[],
            );
            let data = writer.add("NiTransformData", data);
            let interp =
                writer.transform_interpolator([-f32::MAX; 3], [-f32::MAX; 4], -f32::MAX, data);
            let interp = writer.add("NiTransformInterpolator", interp);
            let mut sequence = Buf::default();
            sequence
                .u32(writer.string(name))
                .u32(1)
                .u32(1)
                .i32(interp)
                .i32(-1)
                .u8(0);
            sequence
                .u32(writer.string("Hinge"))
                .u32(u32::MAX)
                .u32(writer.string("NiTransformController"))
                .u32(u32::MAX)
                .u32(u32::MAX);
            sequence
                .f32(1.0)
                .i32(-1)
                .u32(2)
                .f32(frequency)
                .f32(5.0)
                .f32(5.0 + duration);
            sequence.i32(manager).u32(u32::MAX).u16(0);
            sequences.push(writer.add("NiControllerSequence", sequence.0));
        }
        let mut bytes = Buf::default();
        bytes
            .i32(-1)
            .u16(manager_flags)
            .f32(1.0)
            .f32(0.0)
            .f32(0.0)
            .f32(3.0)
            .i32(root)
            .bool(false)
            .refs(&sequences)
            .i32(-1);
        writer.fill(manager, bytes.0);
        let bytes = writer.ni_node("Hinge", [10.0, 20.0, 0.0], IDENTITY, 1.0, &[]);
        writer.fill(hinge, bytes);
        let bytes = writer.ni_node_with_controller(
            "Root",
            [7.0, 0.0, 0.0],
            IDENTITY,
            1.0,
            &[hinge],
            manager,
        );
        writer.fill(root, bytes);
        writer.roots = vec![root];
        Arc::new(ObjectAnimation::from_file(&soe_nif::parse(&writer.finish()).unwrap()).unwrap())
    }

    #[test]
    fn door_sequences_bind_offset_child_hinges_and_use_separate_authored_durations() {
        let asset = door_asset(true, 8);
        assert_eq!(asset.sequence_duration("Open"), Some(1.0));
        assert_eq!(asset.sequence_duration("close"), Some(3.0));
        let rest = asset.rest_pose();
        let mut door = DoorAnimation::new(asset, false).unwrap();
        door.set_opened(true);
        assert!(door.advance(0.5));
        let half = door.pose();
        assert_eq!(half.transforms[0], rest.transforms[0]);
        assert_eq!(
            half.transforms[1].translation,
            rest.transforms[1].translation
        );
        assert_ne!(half.transforms[1].rotation, rest.transforms[1].rotation);
        door.advance(1.0);
        let open = door.pose();
        assert_eq!(door.fraction(), 1.0);
        assert!(!door.advance(1e9));
        assert_eq!(door.pose(), open);
        door.set_opened(false);
        door.advance(1.5);
        assert_eq!(door.fraction(), 0.5);
        assert_eq!(door.pose(), half);
        door.set_opened(true);
        assert_eq!(door.pose(), half);
        for delta in [f32::NAN, f32::INFINITY, -1.0, 0.0] {
            assert!(!door.advance(delta));
            assert_eq!(door.pose(), half);
        }
    }

    #[test]
    fn door_with_one_sequence_holds_authored_pose_when_the_opposite_is_missing() {
        let asset = door_asset(false, 8);
        let mut door = DoorAnimation::new(asset.clone(), false).unwrap();
        door.set_opened(true);
        door.advance(1.0);
        door.set_opened(false);
        assert!(!door.advance(1e9));
        assert_eq!(door.pose(), asset.rest_pose());
        assert!(DoorAnimation::new(Arc::new(self::asset(0)), false).is_none());
    }

    #[test]
    fn door_with_inactive_manager_holds_the_authored_pose_without_a_root_fallback() {
        let asset = door_asset(true, 0);
        assert!(!asset.is_animated());
        assert!(asset.has_sequences());
        assert_eq!(asset.sequence_duration("Open"), None);
        let mut door = DoorAnimation::new(asset.clone(), false).unwrap();
        door.set_opened(true);
        assert!(!door.advance(1e9));
        assert_eq!(door.pose(), asset.rest_pose());
    }

    fn asset(cycle: u16) -> ObjectAnimation {
        let mut writer = NifWriter::new(100);
        let root = writer.reserve("NiNode");
        let data =
            writer.transform_data_linear(&[], &[(0.0, [0.0; 3]), (2.0, [20.0, 0.0, 0.0])], &[]);
        let data = writer.add("NiTransformData", data);
        let interp = writer.transform_interpolator([0.0; 3], [1.0, 0.0, 0.0, 0.0], 1.0, data);
        let interp = writer.add("NiTransformInterpolator", interp);
        let controller =
            writer.transform_controller(-1, 8 | (cycle << 1), 1.0, 0.0, 0.0, 2.0, root, interp);
        let controller = writer.add("NiTransformController", controller);
        let node = writer.ni_node_with_controller("Root", [0.0; 3], IDENTITY, 1.0, &[], controller);
        writer.fill(root, node);
        writer.roots = vec![root];
        let file = soe_nif::parse(&writer.finish()).unwrap();
        ObjectAnimation::from_file(&file).unwrap()
    }

    #[test]
    fn update_samples_thirty_minutes_with_steps_from_one_to_five_hundred_ms() {
        let budget = std::time::Duration::from_secs(1);
        let started = std::time::Instant::now();
        for cycle in 0..3 {
            let asset = asset(cycle);
            let mut clock = ObjectAnimationClock::default();
            let mut elapsed_ms = 0_u32;
            let mut step = 1_u32;
            while elapsed_ms < 1_800_000 {
                let delta_ms = step.min(1_800_000 - elapsed_ms);
                let pose = clock.update(&asset, delta_ms as f32 / 1000.0, None);
                let x = pose.transforms[0].translation.x;
                assert!(x.is_finite() && (0.0..=20.0).contains(&x));
                elapsed_ms += delta_ms;
                let seconds = f64::from(elapsed_ms) / 1000.0;
                let local = match cycle {
                    0 if seconds <= 2.0 => seconds,
                    0 => seconds % 2.0,
                    1 => {
                        let leg = seconds % 4.0;
                        if leg <= 2.0 {
                            leg
                        } else {
                            4.0 - leg
                        }
                    }
                    _ => seconds.min(2.0),
                };
                assert!(
                    (f64::from(x) - 10.0 * local).abs() < 0.01,
                    "cycle {cycle} at {elapsed_ms} ms: {x}"
                );
                step = step % 500 + 1;
            }
            assert!((clock.elapsed - 1800.0).abs() < 0.001);
            let pose = clock.update(&asset, 0.5, None);
            let x = pose.transforms[0].translation.x;
            if cycle == 2 {
                assert_eq!(x, 20.0);
            } else {
                assert!((x - 5.0).abs() < 0.01, "cycle {cycle}: {x}");
            }
        }
        assert!(started.elapsed() < budget);
    }

    #[test]
    fn update_skips_nonfinite_and_negative_deltas_and_reset_restarts_playback() {
        let asset = asset(0);
        let mut clock = ObjectAnimationClock::default();
        let initial = clock.update(&asset, 0.5, None);
        for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
            assert_eq!(clock.update(&asset, delta, None), initial);
        }
        clock.reset();
        assert_eq!(clock.update(&asset, 0.0, None), asset.sample(0.0, None));
    }

    #[test]
    fn update_clock_advances_small_steps_after_a_billion_seconds() {
        let asset = asset(0);
        let mut clock = ObjectAnimationClock { elapsed: 1e9 };
        let before = clock.elapsed;
        clock.update(&asset, 0.001, None);
        assert!(clock.elapsed > before);
    }
}

/// One shape sharing an immutable NIF animation asset with sibling shapes.
#[derive(Debug, Clone)]
pub struct AnimatedSurface {
    pub asset: Arc<ObjectAnimation>,
    pub block: usize,
    pub shader: Option<usize>,
    pub skin: Option<Skin>,
    pub placement: Transform,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub alpha: f32,
}

impl AnimatedSurface {
    /// Bounds of the initial rendered pose, including skinning and placement.
    pub fn bounds(
        &self,
        arrays: &crate::mesh::SurfaceArrays,
        pose: &ObjectPose,
    ) -> crate::mesh::Aabb {
        let bones = self.skin_transforms(pose);
        let shape = pose
            .transforms
            .get(self.block)
            .copied()
            .unwrap_or(Transform::IDENTITY);
        let transform = if self.skin.is_some() {
            self.placement
        } else {
            self.placement.then(&shape)
        };
        let transform = Affine::from_nif(&transform).to_godot();
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for (index, point) in arrays.positions.iter().enumerate() {
            let mut value = *point;
            if !bones.is_empty() && arrays.is_skinned() {
                value = [0.0; 3];
                for slot in 0..4 {
                    let offset = index * 4 + slot;
                    let bone = arrays
                        .bones
                        .get(offset)
                        .and_then(|b| usize::try_from(*b).ok())
                        .and_then(|i| bones.get(i));
                    if let (Some(bone), Some(weight)) = (bone, arrays.weights.get(offset)) {
                        let transformed = bone.apply(*point);
                        for axis in 0..3 {
                            value[axis] += transformed[axis] * weight;
                        }
                    }
                }
            }
            let value = transform.apply(value);
            for axis in 0..3 {
                min[axis] = min[axis].min(value[axis]);
                max[axis] = max[axis].max(value[axis]);
            }
        }
        crate::mesh::Aabb {
            min: crate::units::GodotPos::new(min[0], min[1], min[2]),
            max: crate::units::GodotPos::new(max[0], max[1], max[2]),
        }
    }

    /// Final skin transforms in Godot space, indexed in the mesh bone order.
    pub fn skin_transforms(&self, pose: &ObjectPose) -> Vec<Affine> {
        self.skin.as_ref().map_or_else(Vec::new, |skin| {
            skin.bones
                .iter()
                .map(|bone| {
                    bone.node.get().and_then(|i| pose.transforms.get(i)).map_or(
                        Affine::IDENTITY,
                        |model| {
                            Affine::from_nif(model)
                                .then(&Affine::from_nif_skin(&bone.inverse_bind))
                                .to_godot()
                        },
                    )
                })
                .collect()
        })
    }

    /// Apply sampled shader fields to a fresh copy of the authored material.
    pub fn material(&self, base: &MaterialDesc, pose: &ObjectPose) -> MaterialDesc {
        let mut desc = base.clone();
        for property in &pose.properties {
            if Some(property.target) != self.shader {
                continue;
            }
            use PropertyChannel as P;
            let value = property.value.x;
            match property.channel {
                P::EmissiveColor => desc.emissive_color = property.value.to_array(),
                P::EmissiveMultiple => desc.emissive_multiple = value,
                _ => {}
            }
            if let Some(effect) = &mut desc.effect {
                match property.channel {
                    P::Alpha => effect.params.base_color[3] = value,
                    P::EmissiveMultiple => effect.params.base_color_scale = value,
                    P::EmissiveColor => {
                        effect.params.base_color[..3].copy_from_slice(&property.value.to_array());
                    }
                    P::UOffset => effect.uv_offset[0] = value,
                    P::VOffset => effect.uv_offset[1] = value,
                    P::UScale => effect.uv_scale[0] = value,
                    P::VScale => effect.uv_scale[1] = value,
                    P::FalloffStartAngle => effect.params.falloff_start_angle = value,
                    P::FalloffStopAngle => effect.params.falloff_stop_angle = value,
                    P::FalloffStartOpacity => effect.params.falloff_start_opacity = value,
                    P::FalloffStopOpacity => effect.params.falloff_stop_opacity = value,
                    _ => {}
                }
            }
        }
        desc
    }
}
