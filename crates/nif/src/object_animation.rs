// SPDX-License-Identifier: GPL-3.0-or-later

//! Object sequence binding and playback from nifly Animation.hpp/Animation.cpp,
//! nif.xml ControlledBlock and OpenMW nifosg/controller.cpp. Extra animation
//! blocks retain their original bytes in NifFile, decoded here on demand.

use crate::blocks::{AvObject, Block};
use crate::controller::{self, ControllerFlags, CycleType, TimeController};
use crate::stream::Stream;
use crate::{BlockRef, Error, NifFile, Result, Transform, Vec3};

/// One sampled property value. The target is the property block index.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyValue {
    pub target: usize,
    pub channel: PropertyChannel,
    pub value: Vec3,
}

/// Material fields animated by the supported shader and texture controllers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyChannel {
    Alpha,
    EmissiveMultiple,
    EmissiveColor,
    SpecularColor,
    UOffset,
    VOffset,
    UScale,
    VScale,
    UvRotation,
    FalloffStartAngle,
    FalloffStopAngle,
    FalloffStartOpacity,
    FalloffStopOpacity,
}

#[derive(Debug, Clone)]
enum Track {
    Transform(
        controller::TransformInterpolator,
        Option<controller::TransformData>,
    ),
    Float(f32, controller::KeyGroup<f32>, PropertyChannel),
    Color(Vec3, controller::KeyGroup<Vec3>, PropertyChannel),
}

#[derive(Debug, Clone)]
struct Channel {
    target: usize,
    time: TimeController,
    track: Track,
}

#[derive(Debug, Clone)]
struct Sequence {
    name: String,
    channels: Vec<Channel>,
    accumulation_root: Option<usize>,
}

/// Immutable controller tracks and the scene hierarchy shared by placements.
#[derive(Debug, Clone)]
pub struct ObjectAnimation {
    local: Vec<Transform>,
    hierarchy: Vec<(usize, Option<usize>)>,
    standalone: Vec<Channel>,
    sequences: Vec<Sequence>,
    default_sequences: Vec<usize>,
}

/// Model-space node/shape transforms and material values at one playback time.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectPose {
    pub transforms: Vec<Transform>,
    pub properties: Vec<PropertyValue>,
}

/// Node or shape local placement, including its ObjectNET controller link.
pub fn av_object(block: &Block) -> Option<&AvObject> {
    match block {
        Block::Node(node) => Some(&node.av),
        Block::TriShape(shape) => Some(&shape.av),
        Block::BsTriShape(shape) => Some(&shape.av),
        _ => None,
    }
}

fn raw(block: &Block) -> Option<(&str, &[u8])> {
    match block {
        Block::Unknown { type_name, data } => Some((type_name, data)),
        _ => None,
    }
}

fn finish(s: &Stream<'_>, index: usize, kind: &str, size: usize) -> Result<()> {
    if s.remaining() != 0 {
        return Err(Error::BlockSize {
            index,
            type_name: kind.to_owned(),
            expected_end: size,
            actual_end: s.pos(),
        });
    }
    Ok(())
}

fn invalid(block: usize, reason: impl Into<String>) -> Error {
    Error::InvalidData {
        block,
        reason: reason.into(),
    }
}

fn cycle(value: u32, block: usize) -> Result<CycleType> {
    match value {
        0 => Ok(CycleType::Loop),
        1 => Ok(CycleType::Reverse),
        2 => Ok(CycleType::Clamp),
        _ => Err(invalid(block, format!("unknown sequence cycle {value}"))),
    }
}

fn property_channel(kind: &str, value: u32) -> Option<PropertyChannel> {
    use PropertyChannel as P;
    match (kind, value) {
        ("BSEffectShaderPropertyFloatController", 0)
        | ("BSLightingShaderPropertyFloatController", 11) => Some(P::EmissiveMultiple),
        ("NiAlphaController", _)
        | ("BSEffectShaderPropertyFloatController", 5)
        | ("BSLightingShaderPropertyFloatController", 12) => Some(P::Alpha),
        ("BSEffectShaderPropertyFloatController", 6)
        | ("BSLightingShaderPropertyFloatController", 20)
        | ("NiTextureTransformController", 0) => Some(P::UOffset),
        ("BSEffectShaderPropertyFloatController", 7)
        | ("BSLightingShaderPropertyFloatController", 21)
        | ("NiTextureTransformController", 3) => Some(P::UScale),
        ("BSEffectShaderPropertyFloatController", 8)
        | ("BSLightingShaderPropertyFloatController", 22)
        | ("NiTextureTransformController", 1) => Some(P::VOffset),
        ("BSEffectShaderPropertyFloatController", 9)
        | ("BSLightingShaderPropertyFloatController", 23)
        | ("NiTextureTransformController", 4) => Some(P::VScale),
        ("BSEffectShaderPropertyFloatController", 1) => Some(P::FalloffStartAngle),
        ("BSEffectShaderPropertyFloatController", 2) => Some(P::FalloffStopAngle),
        ("BSEffectShaderPropertyFloatController", 3) => Some(P::FalloffStartOpacity),
        ("BSEffectShaderPropertyFloatController", 4) => Some(P::FalloffStopOpacity),
        ("BSEffectShaderPropertyColorController", 0)
        | ("BSLightingShaderPropertyColorController", 1)
        | ("NiMaterialColorController", 3) => Some(P::EmissiveColor),
        ("BSLightingShaderPropertyColorController", 0) | ("NiMaterialColorController", 2) => {
            Some(P::SpecularColor)
        }
        ("NiTextureTransformController", 2) => Some(P::UvRotation),
        _ => None,
    }
}

struct Binding {
    time: TimeController,
    interpolator: BlockRef,
    channel: Option<PropertyChannel>,
}

fn binding(file: &NifFile, index: usize) -> Result<Option<Binding>> {
    let Some(block) = file.blocks.get(index) else {
        return Err(Error::BadRef {
            block: index,
            reference: index as i32,
        });
    };
    if let Block::TransformController(c) = block {
        return Ok(Some(Binding {
            time: c.time,
            interpolator: c.interpolator,
            channel: None,
        }));
    }
    if let Block::MaterialColorController(c) = block {
        return Ok(Some(Binding {
            time: c.time,
            interpolator: c.interpolator,
            channel: property_channel("NiMaterialColorController", c.target_color as u32),
        }));
    }
    let Some((kind, bytes)) = raw(block) else {
        return Ok(None);
    };
    if !matches!(
        kind,
        "NiAlphaController"
            | "NiTextureTransformController"
            | "BSEffectShaderPropertyFloatController"
            | "BSLightingShaderPropertyFloatController"
            | "BSEffectShaderPropertyColorController"
            | "BSLightingShaderPropertyColorController"
            | "NiMultiTargetTransformController"
            | "BSLagBoneController"
    ) {
        return Ok(None);
    }
    let mut s = Stream::new(bytes);
    let time = controller::read_time_controller(&mut s)?;
    let (interpolator, channel) = match kind {
        "NiMultiTargetTransformController" => {
            let count = s.counted16(4)?;
            for _ in 0..count {
                s.block_ref()?;
            }
            (BlockRef::NONE, None)
        }
        "BSLagBoneController" => {
            s.f32()?;
            s.f32()?;
            s.f32()?;
            (BlockRef::NONE, None)
        }
        _ => {
            let interpolator = s.block_ref()?;
            let variable = if kind == "NiAlphaController" {
                0
            } else {
                if kind == "NiTextureTransformController" {
                    s.bool()?;
                    s.u32()?;
                }
                s.u32()?
            };
            (interpolator, property_channel(kind, variable))
        }
    };
    finish(&s, index, kind, bytes.len())?;
    Ok(Some(Binding {
        time,
        interpolator,
        channel,
    }))
}

fn track(
    file: &NifFile,
    reference: BlockRef,
    channel: Option<PropertyChannel>,
) -> Result<Option<Track>> {
    let Some(index) = reference.get() else {
        return Ok(None);
    };
    let Some(block) = file.blocks.get(index) else {
        return Err(Error::BadRef {
            block: index,
            reference: reference.0,
        });
    };
    match block {
        Block::TransformInterpolator(i) => {
            let data = match file.get(i.data) {
                Some(Block::TransformData(data)) => Some(data.clone()),
                _ => None,
            };
            Ok(Some(Track::Transform(*i, data)))
        }
        Block::Point3Interpolator(i) => {
            let Some(channel) = channel else {
                return Ok(None);
            };
            let keys = match file.get(i.data) {
                Some(Block::PosData(keys)) => keys.clone(),
                _ => controller::KeyGroup::default(),
            };
            Ok(Some(Track::Color(i.value, keys, channel)))
        }
        _ => {
            let Some(("NiFloatInterpolator", bytes)) = raw(block) else {
                return Ok(None);
            };
            let mut s = Stream::new(bytes);
            let value = s.f32()?;
            let data = s.block_ref()?;
            finish(&s, index, "NiFloatInterpolator", bytes.len())?;
            let Some(channel) = channel else {
                return Ok(None);
            };
            let keys = match file.get(data) {
                Some(Block::FloatData(keys)) => keys.clone(),
                _ => controller::KeyGroup::default(),
            };
            Ok(Some(Track::Float(value, keys, channel)))
        }
    }
}

fn sequence(file: &NifFile, index: usize, cumulative: bool) -> Result<Sequence> {
    let Some(("NiControllerSequence", bytes)) = file.blocks.get(index).and_then(raw) else {
        return Err(invalid(index, "manager link is not a NiControllerSequence"));
    };
    let mut s = Stream::new(bytes);
    let name = s.string_index(&file.header.strings)?;
    let count = s.counted(29)?;
    s.u32()?;
    let mut links = Vec::with_capacity(count);
    for _ in 0..count {
        let interpolator = s.block_ref()?;
        let controller = s.block_ref()?;
        let priority = s.u8()?;
        let node = s.string_index(&file.header.strings)?;
        let property = s.string_index(&file.header.strings)?;
        let kind = s.string_index(&file.header.strings)?;
        s.string_index(&file.header.strings)?;
        s.string_index(&file.header.strings)?;
        links.push((interpolator, controller, priority, node, property, kind));
    }
    s.f32()?;
    s.block_ref()?;
    let cycle_type = cycle(s.u32()?, index)?;
    let frequency = s.f32()?;
    let start_time = s.f32()?;
    let stop_time = s.f32()?;
    s.block_ref()?;
    let accumulation_name = s.string_index(&file.header.strings)?;
    if file.header.bs_version > 28 {
        let count = s.counted16(4)?;
        for _ in 0..count {
            s.block_ref()?;
        }
    } else if file.header.bs_version >= 24 {
        s.block_ref()?;
    }
    finish(&s, index, "NiControllerSequence", bytes.len())?;
    let time = TimeController {
        frequency,
        phase: start_time,
        start_time,
        stop_time,
        flags: ControllerFlags {
            active: true,
            cycle_type,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut channels = Vec::new();
    links.sort_by_key(|link| link.2);
    for (interpolator, controller, _, node, property, kind) in links {
        let bound = controller
            .get()
            .map(|i| binding(file, i))
            .transpose()?
            .flatten();
        let named = file
            .blocks
            .iter()
            .position(|block| av_object(block).is_some_and(|av| av.name() == node));
        let target = if property.is_empty() {
            named.or_else(|| bound.as_ref().and_then(|b| b.time.target.get()))
        } else {
            named
                .and_then(|i| shader_ref(&file.blocks[i]).get())
                .or_else(|| bound.as_ref().and_then(|b| b.time.target.get()))
        };
        let Some(target) = target else { continue };
        if target >= file.blocks.len() {
            return Err(Error::BadRef {
                block: index,
                reference: target as i32,
            });
        }
        let channel = bound
            .as_ref()
            .and_then(|b| b.channel)
            .or_else(|| property_channel(&kind, 0));
        if let Some(track) = track(file, interpolator, channel)? {
            channels.push(Channel {
                target,
                time,
                track,
            });
        }
    }
    let accumulation_root = cumulative
        .then(|| {
            file.blocks
                .iter()
                .position(|b| av_object(b).is_some_and(|av| av.name() == accumulation_name))
        })
        .flatten();
    Ok(Sequence {
        name,
        channels,
        accumulation_root,
    })
}

/// Shader property associated with a geometry block.
pub fn shader_ref(block: &Block) -> BlockRef {
    match block {
        Block::TriShape(shape) => shape.shader,
        Block::BsTriShape(shape) => shape.shader,
        _ => BlockRef::NONE,
    }
}

impl ObjectAnimation {
    /// Resolve manager sequences and active autonomous controllers.
    ///
    /// # Errors
    /// Invalid counts, strings, links, block lengths and scene cycles are errors.
    pub fn from_file(file: &NifFile) -> Result<Self> {
        let mut out = Self {
            local: file
                .blocks
                .iter()
                .map(|b| av_object(b).map_or(Transform::IDENTITY, |av| av.transform))
                .collect(),
            hierarchy: Vec::new(),
            standalone: Vec::new(),
            sequences: Vec::new(),
            default_sequences: Vec::new(),
        };
        for root in &file.roots {
            out.visit(file, *root, None, &mut Vec::new())?;
        }
        for (index, block) in file.blocks.iter().enumerate() {
            if let Some(("NiControllerManager", bytes)) = raw(block) {
                let mut s = Stream::new(bytes);
                let time = controller::read_time_controller(&mut s)?;
                let cumulative = s.bool()?;
                let sequences = s.block_refs()?;
                s.block_ref()?;
                finish(&s, index, "NiControllerManager", bytes.len())?;
                if time.flags.active {
                    let start = out.sequences.len();
                    for link in sequences {
                        if let Some(i) = link.get() {
                            out.sequences.push(sequence(file, i, cumulative)?);
                        }
                    }
                    if let Some(default) = default_sequence(&out.sequences[start..]) {
                        out.default_sequences.push(start + default);
                    }
                }
            } else if let Some(bound) = binding(file, index)? {
                if bound.time.flags.active && !bound.time.flags.manager_controlled {
                    if let (Some(target), Some(track)) = (
                        bound.time.target.get(),
                        track(file, bound.interpolator, bound.channel)?,
                    ) {
                        if target >= file.blocks.len() {
                            return Err(Error::BadRef {
                                block: index,
                                reference: target as i32,
                            });
                        }
                        out.standalone.push(Channel {
                            target,
                            time: bound.time,
                            track,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    fn visit(
        &mut self,
        file: &NifFile,
        reference: BlockRef,
        parent: Option<usize>,
        stack: &mut Vec<usize>,
    ) -> Result<()> {
        let Some(index) = reference.get() else {
            return Ok(());
        };
        if stack.contains(&index) || stack.len() >= 256 {
            return Err(Error::Cycle { block: index });
        }
        let Some(block) = file.blocks.get(index) else {
            return Err(Error::BadRef {
                block: parent.unwrap_or(index),
                reference: reference.0,
            });
        };
        if av_object(block).is_none() {
            return Ok(());
        }
        self.hierarchy.push((index, parent));
        stack.push(index);
        if let Block::Node(node) = block {
            for child in &node.children {
                self.visit(file, *child, Some(index), stack)?;
            }
        }
        stack.pop();
        Ok(())
    }

    /// Whether this asset has any executable controller tracks.
    pub fn is_animated(&self) -> bool {
        !self.standalone.is_empty() || self.sequences.iter().any(|s| !s.channels.is_empty())
    }

    /// Names authored by the active controller managers, in file order.
    pub fn sequence_names(&self) -> impl Iterator<Item = &str> {
        self.sequences.iter().map(|s| s.name.as_str())
    }

    /// Sample autonomous controllers and a named sequence, or the default idle.
    /// An unknown explicit name samples only autonomous controllers.
    pub fn sample(&self, seconds: f32, name: Option<&str>) -> ObjectPose {
        let mut pose = ObjectPose {
            transforms: self.local.clone(),
            properties: Vec::new(),
        };
        let selected: Vec<usize> = name.map_or_else(
            || self.default_sequences.clone(),
            |name| {
                self.sequences
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.name.eq_ignore_ascii_case(name).then_some(i))
                    .collect()
            },
        );
        let channels = self.standalone.iter().chain(
            selected
                .iter()
                .flat_map(|i| self.sequences[*i].channels.iter()),
        );
        for channel in channels {
            match &channel.track {
                Track::Transform(interp, data) => {
                    let mut value = controller::evaluate_transform(
                        &channel.time,
                        interp,
                        data.as_ref(),
                        seconds,
                    );
                    let base = self.local[channel.target];
                    // CONTEXT: Gamebryo invalid channels use -FLT_MAX, retaining the authored pose.
                    if !valid(value.translation.x)
                        || !valid(value.translation.y)
                        || !valid(value.translation.z)
                    {
                        value.translation = base.translation;
                    }
                    if !valid(interp.default_value.rotation.w)
                        && data
                            .as_ref()
                            .is_none_or(|d| matches!(d.rotations, controller::RotationKeys::None))
                    {
                        value.rotation = base.rotation;
                    }
                    if !valid(value.scale) {
                        value.scale = base.scale;
                    }
                    pose.transforms[channel.target] = value;
                }
                Track::Float(default, keys, field) => {
                    let value = controller::evaluate_track(
                        keys,
                        controller::evaluate_time(&channel.time, seconds),
                    )
                    .unwrap_or(*default);
                    if valid(value) {
                        pose.properties.push(PropertyValue {
                            target: channel.target,
                            channel: *field,
                            value: Vec3::new(value, 0.0, 0.0),
                        });
                    }
                }
                Track::Color(default, keys, field) => {
                    let value = controller::evaluate_track(
                        keys,
                        controller::evaluate_time(&channel.time, seconds),
                    )
                    .unwrap_or(*default);
                    if valid(value.x) && valid(value.y) && valid(value.z) {
                        pose.properties.push(PropertyValue {
                            target: channel.target,
                            channel: *field,
                            value,
                        });
                    }
                }
            }
        }
        for index in selected {
            let sequence = &self.sequences[index];
            if let Some(root) = sequence.accumulation_root {
                if let Some(channel) = sequence.channels.iter().find(|c| c.target == root) {
                    if let Track::Transform(interp, data) = &channel.track {
                        let time = channel.time;
                        let span = f64::from(time.stop_time) - f64::from(time.start_time);
                        if let Some(clock) =
                            controller::controller_clock(&time, seconds).filter(|t| {
                                time.flags.cycle_type == CycleType::Loop
                                    && *t > f64::from(time.stop_time)
                            })
                        {
                            let mut edge = time;
                            edge.flags.cycle_type = CycleType::Clamp;
                            edge.frequency = 1.0;
                            edge.phase = 0.0;
                            let start = controller::evaluate_transform(
                                &edge,
                                interp,
                                data.as_ref(),
                                time.start_time,
                            );
                            let end = controller::evaluate_transform(
                                &edge,
                                interp,
                                data.as_ref(),
                                time.stop_time,
                            );
                            if let Some(inverse) = inverse(start) {
                                let delta = end.then(&inverse);
                                let cycles =
                                    ((clock - f64::from(time.start_time)) / span).floor() as u64;
                                pose.transforms[root] =
                                    repeated(delta, cycles).then(&pose.transforms[root]);
                            }
                        }
                    }
                }
            }
        }
        for (index, parent) in &self.hierarchy {
            if let Some(parent) = parent {
                pose.transforms[*index] = pose.transforms[*parent].then(&pose.transforms[*index]);
            }
        }
        pose
    }
}

fn valid(value: f32) -> bool {
    value.is_finite() && value > -1e30 && value < 1e30
}

fn inverse(transform: Transform) -> Option<Transform> {
    if !valid(transform.scale) || transform.scale.abs() < f32::EPSILON {
        return None;
    }
    let rotation = crate::Mat33 {
        rows: std::array::from_fn(|r| std::array::from_fn(|c| transform.rotation.rows[c][r])),
    };
    let scale = 1.0 / transform.scale;
    let translation = rotation.apply(transform.translation.scale(-scale));
    Some(Transform {
        rotation,
        translation,
        scale,
    })
}

fn repeated(mut transform: Transform, mut count: u64) -> Transform {
    let mut out = Transform::IDENTITY;
    while count > 0 {
        if count & 1 != 0 {
            out = out.then(&transform);
        }
        count >>= 1;
        if count > 0 {
            transform = transform.then(&transform);
        }
    }
    out
}

fn default_sequence(sequences: &[Sequence]) -> Option<usize> {
    ["default", "idle", "loop"]
        .iter()
        .find_map(|name| {
            sequences
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(name))
        })
        .or_else(|| {
            sequences.iter().position(|s| {
                s.channels
                    .first()
                    .is_some_and(|c| c.time.flags.cycle_type == CycleType::Loop)
            })
        })
}
