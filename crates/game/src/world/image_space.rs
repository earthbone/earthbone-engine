// SPDX-License-Identifier: GPL-3.0-or-later

//! Image-space lifetimes and base data from the shipped Papyrus class, xEdit
//! IMAD/IMGS/CELL/WTHR definitions and CommonLibSSE-NG ImageSpaceData.

use std::collections::BTreeSet;

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Reader, Writer};
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{World, T};

pub(crate) const SECTION: super::save_sections::Section<World> = super::save_sections::Section {
    name: "MAG-72",
    write: write_weather_selection,
    read: read_weather_selection,
};

fn write_weather_selection(world: &World, writer: &mut Writer, _pool: &mut ArrayPool) {
    if world.weather_image_space.is_some() {
        world.write_weather_image_space(writer);
    }
}

fn read_weather_selection(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    world.weather_image_space = world.read_weather_image_space(reader)?;
    Ok(())
}

/// Winning IMGS values, in the order of xEdit HNAM, CNAM, TNAM and DNAM.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseImageSpace {
    pub hdr: [f32; 9],
    pub cinematic: [f32; 3],
    pub tint: [f32; 4],
    pub depth: [f32; 3],
    pub depth_flags: u16,
    pub sky_blur_radius: u16,
}

impl Default for BaseImageSpace {
    fn default() -> Self {
        Self {
            hdr: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0],
            cinematic: [1.0; 3],
            tint: [0.0; 4],
            depth: [0.0; 3],
            depth_flags: 0,
            sky_blur_radius: 0,
        }
    }
}

/// Four Skyrim WTHR/IMSP entries, in sunrise, day, sunset, night order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherSelection {
    pub weather: GlobalFormId,
    pub from: u8,
    pub to: u8,
    pub blend: f32,
}

fn floats<const N: usize>(bytes: &[u8]) -> Result<[f32; N], VmError> {
    if bytes.len() != N * 4 {
        return Err(VmError::State("invalid IMGS field size".into()));
    }
    let mut values = [0.0; N];
    for (value, chunk) in values.iter_mut().zip(bytes.as_chunks::<4>().0) {
        *value = f32::from_le_bytes(*chunk);
        if !value.is_finite() {
            return Err(VmError::State("non-finite IMGS field".into()));
        }
    }
    Ok(values)
}

impl BaseImageSpace {
    fn blend(&self, other: &Self, weight: f32) -> Self {
        let mut out = self.clone();
        for (dst, (a, b)) in out.hdr.iter_mut().zip(self.hdr.iter().zip(other.hdr)) {
            *dst = a * (1.0 - weight) + b * weight;
        }
        for (dst, (a, b)) in out
            .cinematic
            .iter_mut()
            .zip(self.cinematic.iter().zip(other.cinematic))
        {
            *dst = a * (1.0 - weight) + b * weight;
        }
        for (dst, (a, b)) in out.tint.iter_mut().zip(self.tint.iter().zip(other.tint)) {
            *dst = a * (1.0 - weight) + b * weight;
        }
        for (dst, (a, b)) in out.depth.iter_mut().zip(self.depth.iter().zip(other.depth)) {
            *dst = a * (1.0 - weight) + b * weight;
        }
        if weight >= 0.5 {
            out.depth_flags = other.depth_flags;
            out.sky_blur_radius = other.sky_blur_radius;
        }
        out
    }
}

impl World {
    /// Select one authored weather and two of its four time entries.
    ///
    /// # Errors
    /// Rejects a missing weather, malformed IMSP or invalid blend before changing state.
    pub fn select_weather_image_space(
        &mut self,
        selection: Option<WeatherSelection>,
    ) -> Result<(), VmError> {
        self.validate_weather_selection(selection)?;
        self.weather_image_space = selection;
        Ok(())
    }

    fn validate_weather_selection(
        &self,
        selection: Option<WeatherSelection>,
    ) -> Result<(), VmError> {
        if let Some(value) = selection {
            if value.from >= 4
                || value.to >= 4
                || !value.blend.is_finite()
                || !(0.0..=1.0).contains(&value.blend)
            {
                return Err(VmError::State(
                    "invalid weather image-space selection".into(),
                ));
            }
            let record = self
                .record(value.weather)
                .filter(|r| r.tag == T(b"WTHR") && !r.is_deleted())
                .ok_or_else(|| VmError::State("missing WTHR image-space source".into()))?;
            if record.find(T(b"IMSP")).is_none_or(|data| data.len() != 16) {
                return Err(VmError::State("invalid WTHR IMSP size".into()));
            }
        }
        Ok(())
    }

    pub(crate) fn write_weather_image_space(&self, writer: &mut Writer) {
        writer.bool(self.weather_image_space.is_some());
        if let Some(selection) = self.weather_image_space {
            write_form_id(writer, selection.weather);
            writer.u8(selection.from);
            writer.u8(selection.to);
            writer.f32(selection.blend);
        }
    }

    pub(crate) fn read_weather_image_space(
        &self,
        reader: &mut FormReader<'_, '_>,
    ) -> Result<Option<WeatherSelection>, VmError> {
        let selection = reader
            .bool()?
            .then(|| -> Result<WeatherSelection, VmError> {
                Ok(WeatherSelection {
                    weather: read_form_id(reader)?,
                    from: reader.u8()?,
                    to: reader.u8()?,
                    blend: reader.f32()?,
                })
            })
            .transpose()?;
        self.validate_weather_selection(selection)?;
        Ok(selection)
    }

    /// The explicitly selected WTHR image space, falling back to the loaded CELL/XCIM.
    ///
    /// # Errors
    /// Rejects malformed winning IMGS data or an invalid selected weather.
    pub fn base_image_space(&self) -> Result<Option<BaseImageSpace>, VmError> {
        let cell = if let Some(id) = self.loaded_cell {
            self.record(id)
                .filter(|r| r.tag == T(b"CELL") && !r.is_deleted())
                .and_then(|r| r.find(T(b"XCIM")))
                .map(|data| {
                    let raw: [u8; 4] = data
                        .try_into()
                        .map_err(|_| VmError::State("invalid CELL XCIM size".into()))?;
                    self.read_linked_base_image_space(id, u32::from_le_bytes(raw))
                })
                .transpose()?
                .flatten()
        } else {
            None
        };
        let cell_base = cell;
        let selection = self.weather_image_space.or_else(|| {
            self.weather_request().map(|request| WeatherSelection {
                weather: request.weather,
                from: 1,
                to: 1,
                blend: 0.0,
            })
        });
        let Some(selection) = selection else {
            return Ok(cell_base);
        };
        let record = self
            .record(selection.weather)
            .filter(|r| r.tag == T(b"WTHR") && !r.is_deleted())
            .ok_or_else(|| VmError::State("missing WTHR image-space source".into()))?;
        let data = record
            .find(T(b"IMSP"))
            .filter(|data| data.len() == 16)
            .ok_or_else(|| VmError::State("invalid WTHR IMSP size".into()))?;
        let slot = |index: u8| -> Result<Option<BaseImageSpace>, VmError> {
            let start = usize::from(index) * 4;
            let raw = u32::from_le_bytes(
                data[start..start + 4]
                    .try_into()
                    .map_err(|_| VmError::State("invalid WTHR IMSP entry".into()))?,
            );
            self.read_linked_base_image_space(selection.weather, raw)
        };
        let from = slot(selection.from)?.or_else(|| cell_base.clone());
        let to = slot(selection.to)?.or_else(|| cell_base.clone());
        Ok(match (from, to) {
            (Some(a), Some(b)) => Some(a.blend(&b, selection.blend)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        })
    }

    fn read_linked_base_image_space(
        &self,
        owner: GlobalFormId,
        raw: u32,
    ) -> Result<Option<BaseImageSpace>, VmError> {
        if raw == 0 {
            return Ok(None);
        }
        let id = self
            .resolve_from(owner, raw)
            .ok_or_else(|| VmError::State("unresolved image-space form link".into()))?;
        self.read_base_image_space(id).map(Some)
    }

    fn read_base_image_space(&self, id: GlobalFormId) -> Result<BaseImageSpace, VmError> {
        let record = self
            .record(id)
            .filter(|r| r.tag == T(b"IMGS") && !r.is_deleted())
            .ok_or_else(|| VmError::State("image-space link does not resolve to IMGS".into()))?;
        let mut base = BaseImageSpace::default();
        if let Some(data) = record.find(T(b"ENAM")) {
            let old = floats::<14>(data)?;
            base.hdr[..5].copy_from_slice(&old[..5]);
            base.hdr[6..8].copy_from_slice(&old[5..7]);
            base.cinematic.copy_from_slice(&old[7..10]);
            base.tint.copy_from_slice(&old[10..14]);
        }
        if let Some(data) = record.find(T(b"HNAM")) {
            base.hdr = floats::<9>(data)?;
        }
        if let Some(data) = record.find(T(b"CNAM")) {
            base.cinematic = floats::<3>(data)?;
        }
        if let Some(data) = record.find(T(b"TNAM")) {
            base.tint = floats::<4>(data)?;
        }
        if let Some(data) = record.find(T(b"DNAM")) {
            if data.len() != 16 {
                return Err(VmError::State("invalid IMGS depth size".into()));
            }
            base.depth = floats::<3>(&data[..12])?;
            base.depth_flags = u16::from_le_bytes([data[12], data[13]]);
            base.sky_blur_radius = u16::from_le_bytes([data[14], data[15]]);
        }
        Ok(base)
    }
}

/// One runtime application. Authored curves remain in its winning IMAD record.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpaceLayer {
    pub modifier: GlobalFormId,
    pub strength: f32,
    pub age: f64,
}

/// One authored IMAD curve sampled at a normalized modifier age.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpaceCurveSample {
    pub channel: [u8; 4],
    pub values: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
struct Fade {
    elapsed: f64,
    duration: f64,
    from: Vec<(GlobalFormId, f32)>,
    to: Option<GlobalFormId>,
}

/// Native lifetime state. Ordinary applications do not join the crossfade chain.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ImageSpaceState {
    ordinary: Vec<ImageSpaceLayer>,
    chain: Vec<ImageSpaceLayer>,
    fade: Option<Fade>,
}

impl ImageSpaceState {
    fn remove(&mut self, modifier: GlobalFormId) {
        self.ordinary.retain(|layer| layer.modifier != modifier);
        self.chain.retain(|layer| layer.modifier != modifier);
        if let Some(fade) = &mut self.fade {
            fade.from.retain(|(id, _)| *id != modifier);
            if fade.to == Some(modifier) {
                fade.to = None;
            }
        }
        if self.chain.is_empty() {
            self.fade = None;
        }
    }

    fn crossfade(&mut self, to: Option<GlobalFormId>, duration: f64) {
        if to.is_none() && self.chain.is_empty() {
            self.fade = None;
            return;
        }
        let from = self
            .chain
            .iter()
            .map(|layer| (layer.modifier, layer.strength))
            .collect();
        if let Some(modifier) = to {
            self.ordinary.retain(|layer| layer.modifier != modifier);
            if let Some(layer) = self
                .chain
                .iter_mut()
                .find(|layer| layer.modifier == modifier)
            {
                layer.age = 0.0;
            } else {
                self.chain.push(ImageSpaceLayer {
                    modifier,
                    strength: 0.0,
                    age: 0.0,
                });
            }
        }
        self.fade = Some(Fade {
            elapsed: 0.0,
            duration,
            from,
            to,
        });
        self.advance_fade(0.0);
    }

    fn advance_fade(&mut self, dt: f64) {
        let Some(fade) = &mut self.fade else { return };
        fade.elapsed = (fade.elapsed + dt).min(fade.duration);
        let t = if fade.duration == 0.0 {
            1.0
        } else {
            fade.elapsed / fade.duration
        };
        for layer in &mut self.chain {
            let from = fade
                .from
                .iter()
                .find(|(id, _)| *id == layer.modifier)
                .map_or(0.0, |(_, value)| f64::from(*value));
            let to = f64::from(u8::from(fade.to == Some(layer.modifier)));
            layer.strength = (from * (1.0 - t) + to * t) as f32;
        }
        if t == 1.0 {
            self.chain.retain(|layer| Some(layer.modifier) == fade.to);
            self.fade = None;
        }
    }
}

fn sample_curve(
    record: &soe_esm::Record,
    header: &[u8],
    channel: [u8; 4],
    count_offset: usize,
    color: bool,
    time: f32,
) -> Result<Option<Vec<f32>>, VmError> {
    let tag = T(&channel);
    let count = u32::from_le_bytes(
        header[count_offset..count_offset + 4]
            .try_into()
            .map_err(|_| VmError::State("truncated IMAD curve count".into()))?,
    ) as usize;
    let mut entries = record.all(tag);
    let Some(data) = entries.next() else {
        if count == 0 {
            return Ok(None);
        }
        return Err(VmError::State("missing IMAD curve data".into()));
    };
    if entries.next().is_some() {
        return Err(VmError::State("duplicate IMAD curve data".into()));
    }
    let stride = if color { 20 } else { 8 };
    if data.len() != count.saturating_mul(stride) || count == 0 {
        return Err(VmError::State("invalid IMAD curve size".into()));
    }
    let mut keys = Vec::with_capacity(count);
    let mut previous = None;
    for key in data.chunks_exact(stride) {
        let (words, _) = key.as_chunks::<4>();
        let values: Vec<f32> = words
            .iter()
            .map(|chunk| {
                let bits = u32::from_le_bytes(*chunk);
                let value = f32::from_bits(bits);
                value
                    .is_finite()
                    .then_some(value)
                    .ok_or_else(|| VmError::State("non-finite IMAD curve value".into()))
            })
            .collect::<Result<_, _>>()?;
        let key_time = values[0];
        if previous.is_some_and(|last| last > key_time) {
            return Err(VmError::State("unordered IMAD curve keys".into()));
        }
        previous = Some(key_time);
        keys.push((key_time, values[1..].to_vec()));
    }
    let sample_time = time.clamp(keys[0].0, keys[keys.len() - 1].0);
    let upper = keys.partition_point(|(key_time, _)| *key_time < sample_time);
    let (left, right) = if upper == 0 {
        (&keys[0], &keys[0])
    } else if upper == keys.len() {
        (&keys[keys.len() - 1], &keys[keys.len() - 1])
    } else {
        (&keys[upper - 1], &keys[upper])
    };
    let amount = if right.0 == left.0 {
        0.0
    } else {
        (sample_time - left.0) / (right.0 - left.0)
    };
    Ok(Some(
        left.1
            .iter()
            .zip(&right.1)
            .map(|(from, to)| from + (to - from) * amount)
            .collect(),
    ))
}

impl World {
    /// Snapshot in application order, with the crossfade chain last.
    pub fn image_space_layers(&self) -> Vec<ImageSpaceLayer> {
        self.image_space
            .ordinary
            .iter()
            .chain(&self.image_space.chain)
            .cloned()
            .collect()
    }

    /// Sample the authored curves for an active modifier at normalized time.
    ///
    /// Float curves use linear interpolation between adjacent keys. Color
    /// curves return red, green, blue and alpha in the record's stored range.
    pub fn image_space_curve_samples(
        &self,
        modifier: GlobalFormId,
        time: f32,
    ) -> Result<Vec<ImageSpaceCurveSample>, VmError> {
        if !time.is_finite() {
            return Err(VmError::State("non-finite image-space curve time".into()));
        }
        let winner = self
            .order
            .winner_ref(modifier)
            .ok_or_else(|| VmError::State("missing image-space modifier".into()))?;
        let record = self.order.record(winner);
        if record.tag != T(b"IMAD") || record.flags & 0x20 != 0 {
            return Err(VmError::State("invalid image-space modifier".into()));
        }
        let header = record
            .find(T(b"DNAM"))
            .filter(|data| data.len() == 244)
            .ok_or_else(|| VmError::State("invalid IMAD data size".into()))?;

        let definitions: &[([u8; 4], usize, bool)] = &[
            (*b"TNAM", 176, true),
            (*b"BNAM", 180, false),
            (*b"VNAM", 184, false),
            (*b"RNAM", 188, false),
            (*b"SNAM", 192, false),
            (*b"UNAM", 196, false),
            (*b"WNAM", 212, false),
            (*b"XNAM", 216, false),
            (*b"YNAM", 220, false),
            (*b"NAM1", 228, false),
            (*b"NAM2", 232, false),
            (*b"NAM3", 236, true),
            (*b"NAM4", 240, false),
        ];
        let mut samples = Vec::new();
        for &(channel, count_offset, color) in definitions {
            sample_curve(record, header, channel, count_offset, color, time)?
                .into_iter()
                .for_each(|values| samples.push(ImageSpaceCurveSample { channel, values }));
        }
        for index in 0..21 {
            for part in 0..2 {
                let channel = [index as u8 + part * 0x40, b'I', b'A', b'D'];
                let count_offset = 8 + index * 8 + usize::from(part) * 4;
                if let Some(values) =
                    sample_curve(record, header, channel, count_offset, false, time)?
                {
                    samples.push(ImageSpaceCurveSample { channel, values });
                }
            }
        }
        Ok(samples)
    }

    /// Return the normalized age used to sample an active layer's curves.
    pub fn image_space_layer_time(&self, layer: &ImageSpaceLayer) -> Result<f32, VmError> {
        match self.image_space_duration(layer.modifier)? {
            Some(duration) if duration > 0.0 => Ok((layer.age / duration).clamp(0.0, 1.0) as f32),
            Some(_) => Ok(1.0),
            None => Ok(0.0),
        }
    }

    fn image_space_duration(&self, modifier: GlobalFormId) -> Result<Option<f64>, VmError> {
        let winner = self
            .order
            .winner_ref(modifier)
            .ok_or_else(|| VmError::State("missing image-space modifier".into()))?;
        let record = self.order.record(winner);
        let data = record
            .find(T(b"DNAM"))
            .filter(|_| record.tag == T(b"IMAD") && record.flags & 0x20 == 0)
            .ok_or_else(|| VmError::State("missing IMAD data".into()))?;
        if data.len() != 244 {
            return Err(VmError::State("invalid IMAD data size".into()));
        }
        let mut reader = Reader::new(data);
        let animated = reader.u32()?;
        let duration = reader.f32()?;
        if animated > 1 || !duration.is_finite() || duration < 0.0 {
            return Err(VmError::State("invalid IMAD lifetime".into()));
        }
        Ok((animated != 0).then_some(f64::from(duration)))
    }

    pub fn apply_image_space(
        &mut self,
        modifier: GlobalFormId,
        strength: f32,
    ) -> Result<(), VmError> {
        self.image_space_duration(modifier)?;
        if !strength.is_finite() {
            return Err(VmError::State("non-finite image-space strength".into()));
        }
        self.image_space.remove(modifier);
        self.image_space.ordinary.push(ImageSpaceLayer {
            modifier,
            strength,
            age: 0.0,
        });
        Ok(())
    }

    pub fn pop_image_space(
        &mut self,
        modifier: GlobalFormId,
        next: GlobalFormId,
        strength: f32,
    ) -> Result<(), VmError> {
        self.image_space_duration(modifier)?;
        self.image_space_duration(next)?;
        if !strength.is_finite() {
            return Err(VmError::State("non-finite image-space strength".into()));
        }
        self.image_space.remove(modifier);
        self.apply_image_space(next, strength)
    }

    pub fn remove_image_space(&mut self, modifier: GlobalFormId) {
        self.image_space.remove(modifier);
    }

    pub fn crossfade_image_space(
        &mut self,
        modifier: Option<GlobalFormId>,
        seconds: f32,
    ) -> Result<(), VmError> {
        if let Some(id) = modifier {
            self.image_space_duration(id)?;
        }
        if !seconds.is_finite() {
            return Err(VmError::State(
                "non-finite image-space fade duration".into(),
            ));
        }
        self.image_space
            .crossfade(modifier, f64::from(seconds.max(0.0)));
        Ok(())
    }

    pub(crate) fn tick_image_space(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds < 0.0 {
            return;
        }
        for layer in self
            .image_space
            .ordinary
            .iter_mut()
            .chain(&mut self.image_space.chain)
        {
            layer.age = (layer.age + seconds).min(f64::MAX);
        }
        self.image_space.advance_fade(seconds);
        let expired: Vec<_> = self
            .image_space_layers()
            .iter()
            .filter_map(|layer| match self.image_space_duration(layer.modifier) {
                Ok(Some(duration)) if layer.age >= duration => Some(layer.modifier),
                Err(_) => Some(layer.modifier),
                _ => None,
            })
            .collect();
        for modifier in expired {
            self.image_space.remove(modifier);
        }
    }

    pub(crate) fn write_image_space(&self, writer: &mut Writer) {
        for layers in [&self.image_space.ordinary, &self.image_space.chain] {
            writer.len(layers.len());
            for layer in layers {
                write_form_id(writer, layer.modifier);
                writer.f32(layer.strength);
                writer.f64(layer.age);
            }
        }
        writer.bool(self.image_space.fade.is_some());
        if let Some(fade) = &self.image_space.fade {
            writer.f64(fade.elapsed);
            writer.f64(fade.duration);
            super::save::write_opt_form_id(writer, fade.to);
            writer.len(fade.from.len());
            for (id, strength) in &fade.from {
                write_form_id(writer, *id);
                writer.f32(*strength);
            }
        }
    }

    pub(crate) fn read_image_space(
        &self,
        reader: &mut FormReader<'_, '_>,
    ) -> Result<ImageSpaceState, VmError> {
        let mut state = ImageSpaceState::default();
        let mut ids = BTreeSet::new();
        for layers in [&mut state.ordinary, &mut state.chain] {
            for _ in 0..reader.len()? {
                let modifier = read_form_id(reader)?;
                let strength = reader.f32()?;
                let age = reader.f64()?;
                self.image_space_duration(modifier)?;
                if !ids.insert(modifier) || !strength.is_finite() || !age.is_finite() || age < 0.0 {
                    return Err(VmError::State("invalid image-space layer".into()));
                }
                layers.push(ImageSpaceLayer {
                    modifier,
                    strength,
                    age,
                });
            }
        }
        if reader.bool()? {
            let elapsed = reader.f64()?;
            let duration = reader.f64()?;
            let to = super::save::read_opt_form_id(reader)?;
            let chain_ids: BTreeSet<_> = state.chain.iter().map(|layer| layer.modifier).collect();
            if !elapsed.is_finite()
                || elapsed < 0.0
                || !duration.is_finite()
                || duration <= elapsed
                || to.is_some_and(|id| !chain_ids.contains(&id))
                || chain_ids.is_empty()
            {
                return Err(VmError::State("invalid image-space fade".into()));
            }
            let mut from = Vec::new();
            let mut seen = BTreeSet::new();
            for _ in 0..reader.len()? {
                let id = read_form_id(reader)?;
                let strength = reader.f32()?;
                if !chain_ids.contains(&id)
                    || !seen.insert(id)
                    || !strength.is_finite()
                    || !(0.0..=1.0).contains(&strength)
                {
                    return Err(VmError::State("invalid image-space fade source".into()));
                }
                from.push((id, strength));
            }
            seen.extend(to);
            if seen != chain_ids {
                return Err(VmError::State(
                    "image-space fade has an unowned layer".into(),
                ));
            }
            state.fade = Some(Fade {
                elapsed,
                duration,
                from,
                to,
            });
            let saved_chain = state.chain.clone();
            state.advance_fade(0.0);
            if state.chain != saved_chain {
                return Err(VmError::State(
                    "image-space fade weights do not match its time".into(),
                ));
            }
        } else if state.chain.len() > 1 || state.chain.iter().any(|layer| layer.strength != 1.0) {
            return Err(VmError::State("invalid settled image-space chain".into()));
        }
        Ok(state)
    }
}
