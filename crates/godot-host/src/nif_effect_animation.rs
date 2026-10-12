// SPDX-License-Identifier: GPL-3.0-or-later

//! Standalone shader float tracks from nifly Animation.cpp and nif.xml's
//! EffectShaderControlledVariable and LightingShaderControlledFloat. OpenMW ControllerFunction supplies cycling.
//! The NIF reader retains these controllers as raw blocks until RND-23.
//! Generate shader evaluation rather than allocating reference scene nodes.

use soe_nif::blocks::Block;
use soe_nif::controller::{Interpolation, KeyGroup};
use soe_nif::stream::Stream;
use soe_nif::{BlockRef, Error, NifFile, Result};

/// One active shader float controller and its float interpolator.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectFloatTrack {
    pub flags: u16,
    pub frequency: f32,
    pub phase: f32,
    pub start: f32,
    pub stop: f32,
    pub channel: u32,
    pub default: f32,
    pub keys: KeyGroup<f32>,
}

fn invalid(block: usize, reason: &str) -> Error {
    Error::InvalidData {
        block,
        reason: format!("effect animation: {reason}"),
    }
}

fn block(file: &NifFile, reference: BlockRef, owner: usize) -> Result<Option<&Block>> {
    if reference.0 < -1 {
        return Err(Error::BadRef {
            block: owner,
            reference: reference.0,
        });
    }
    reference
        .get()
        .map(|index| {
            file.blocks.get(index).ok_or(Error::BadRef {
                block: owner,
                reference: reference.0,
            })
        })
        .transpose()
}

/// Resolve a shape's linked standalone effect or lighting UV controllers.
/// Malformed links, cycles, non-finite tracks and unsupported interpolation
/// return a NIF error before the surface is uploaded.
pub(crate) fn for_shape(file: &NifFile, shape: usize) -> Result<Vec<EffectFloatTrack>> {
    let shader_ref = match file.blocks.get(shape) {
        Some(Block::BsTriShape(s)) => s.shader,
        Some(Block::TriShape(s)) => s.shader,
        _ => return Ok(Vec::new()),
    };
    let (net, lighting) = match block(file, shader_ref, shape)? {
        Some(Block::EffectShader(shader)) => (&shader.net, false),
        Some(Block::LightingShader(shader)) => (&shader.net, true),
        _ => return Ok(Vec::new()),
    };
    let mut reference = net.controller;
    block(file, reference, shape)?;
    let mut visited = Vec::new();
    let mut tracks = Vec::new();
    while let Some(index) = reference.get() {
        if visited.contains(&index) {
            return Err(Error::Cycle { block: index });
        }
        visited.push(index);
        let Some(Block::Unknown { type_name, data }) = block(file, reference, shape)? else {
            break;
        };
        let mut stream = Stream::new(data);
        reference = stream.block_ref()?;
        block(file, reference, index)?;
        let expected = if lighting {
            "BSLightingShaderPropertyFloatController"
        } else {
            "BSEffectShaderPropertyFloatController"
        };
        if type_name != expected {
            continue;
        }
        let flags = stream.u16()?;
        let frequency = stream.f32()?;
        let phase = stream.f32()?;
        let start = stream.f32()?;
        let stop = stream.f32()?;
        let target = stream.block_ref()?;
        let interpolator = stream.block_ref()?;
        let raw_channel = stream.u32()?;
        let channel = if lighting {
            match raw_channel {
                20..=23 => raw_channel - 14,
                _ => continue,
            }
        } else {
            raw_channel
        };
        if stream.remaining() != 0 {
            return Err(invalid(index, "controller has trailing fields"));
        }
        if flags & 8 == 0 || flags & 32 != 0 || target != shader_ref || channel > 9 {
            continue;
        }
        if ![frequency, phase, start, stop, stop - start]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(invalid(index, "controller time must be finite"));
        }
        let Some(Block::Unknown { type_name, data }) = block(file, interpolator, index)? else {
            return Err(invalid(index, "missing float interpolator"));
        };
        if type_name != "NiFloatInterpolator" {
            return Err(invalid(index, "unsupported float interpolator"));
        }
        let mut stream = Stream::new(data);
        let default = stream.f32()?;
        let data_ref = stream.block_ref()?;
        if stream.remaining() != 0 {
            return Err(invalid(index, "interpolator has trailing fields"));
        }
        let keys = match block(file, data_ref, index)? {
            None => KeyGroup::default(),
            Some(Block::FloatData(keys)) => keys.clone(),
            _ => return Err(invalid(index, "interpolator data is not NiFloatData")),
        };
        if keys.keys.is_empty() && (!default.is_finite() || default <= -f32::MAX) {
            continue;
        }
        if matches!(
            keys.interpolation,
            Interpolation::Tbc | Interpolation::XyzRotation
        ) {
            return Err(invalid(index, "unsupported float key interpolation"));
        }
        if keys.keys.iter().any(|key| {
            ![key.time, key.value, key.in_tan, key.out_tan]
                .iter()
                .all(|v| v.is_finite())
        }) || keys
            .keys
            .windows(2)
            .any(|pair| pair[1].time <= pair[0].time)
        {
            return Err(invalid(index, "float keys must be finite and ordered"));
        }
        tracks.push(EffectFloatTrack {
            flags,
            frequency,
            phase,
            start,
            stop,
            channel,
            default,
            keys,
        });
    }
    Ok(tracks)
}

pub(crate) fn identity(tracks: &[EffectFloatTrack]) -> String {
    format!(
        "{}{}{}",
        functions(tracks),
        assignments(tracks, true),
        assignments(tracks, false)
    )
}

fn number(value: f32) -> String {
    format!("{value:e}")
}

pub(crate) fn functions(tracks: &[EffectFloatTrack]) -> String {
    let mut source = String::new();
    for (index, track) in tracks.iter().enumerate() {
        source.push_str(&format!(
            "float effect_track_{index}(float elapsed) {{\nfloat t = elapsed * {} + {};\n",
            number(track.frequency),
            number(track.phase)
        ));
        let start = number(track.start);
        let stop = number(track.stop);
        let span = number(track.stop - track.start);
        if track.stop <= track.start {
            source.push_str(&format!("t = {start};\n"));
        } else {
            source.push_str(&format!("if (t < {start} || t > {stop}) {{\n"));
            match (track.flags >> 1) & 3 {
                0 => source.push_str(&format!("t = {start} + mod(t - {start}, {span});\n")),
                1 => source.push_str(&format!(
                    "float cycle = mod(t - {start}, 2.0 * {span});\nt = {start} + min(cycle, 2.0 * {span} - cycle);\n"
                )),
                _ => source.push_str(&format!("t = clamp(t, {start}, {stop});\n")),
            }
            source.push_str("}\n");
        }
        if track.flags & 16 != 0 {
            source.push_str(&format!("t = {start} + {stop} - t;\n"));
        }
        if let Some(first) = track.keys.keys.first() {
            source.push_str(&format!(
                "if (t <= {}) {{ return {}; }}\n",
                number(first.time),
                number(first.value)
            ));
            for pair in track.keys.keys.windows(2) {
                let a = &pair[0];
                let b = &pair[1];
                source.push_str(&format!(
                    "if (t <= {}) {{ float f = (t - {}) / {};\n",
                    number(b.time),
                    number(a.time),
                    number(b.time - a.time)
                ));
                match track.keys.interpolation {
                    Interpolation::Quadratic => source.push_str(&format!(
                        "return (2.0*f*f*f-3.0*f*f+1.0)*{} + (-2.0*f*f*f+3.0*f*f)*{} + (f*f*f-2.0*f*f+f)*{} + (f*f*f-f*f)*{};\n",
                        number(a.value), number(b.value), number(a.out_tan), number(b.in_tan)
                    )),
                    Interpolation::Constant => source.push_str(&format!(
                        "return f > 0.5 ? {} : {};\n", number(b.value), number(a.value)
                    )),
                    _ => source.push_str(&format!("return mix({}, {}, f);\n", number(a.value), number(b.value))),
                }
                source.push_str("}\n");
            }
            let last = track
                .keys
                .keys
                .last()
                .map_or(track.default, |key| key.value);
            source.push_str(&format!("return {};\n", number(last)));
        } else {
            source.push_str(&format!("return {};\n", number(track.default)));
        }
        source.push_str("}\n");
    }
    source
}

pub(crate) fn assignments(tracks: &[EffectFloatTrack], vertex: bool) -> String {
    let mut source = String::new();
    for (index, track) in tracks.iter().enumerate() {
        let target = match (vertex, track.channel) {
            (true, 1) => "animated_falloff.x",
            (true, 2) => "animated_falloff.y",
            (true, 3) => "animated_falloff.z",
            (true, 4) => "animated_falloff.w",
            (false, 0) => "animated_multiple",
            (false, 5) => "animated_color.a",
            (false, 6) => "animated_offset.x",
            (false, 7) => "animated_scale.x",
            (false, 8) => "animated_offset.y",
            (false, 9) => "animated_scale.y",
            _ => continue,
        };
        let sample = format!("effect_track_{index}(animation_time >= 0.0 ? animation_time : TIME)");
        let value = if vertex && matches!(track.channel, 1 | 2) {
            format!("cos(radians({sample}))")
        } else {
            sample
        };
        source.push_str(&format!("{target} = {value};\n"));
    }
    source
}
