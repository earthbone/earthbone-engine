// SPDX-License-Identifier: GPL-3.0-or-later

//! ImageSpaceModifier lifetimes from the shipped Papyrus class and xEdit IMAD.

use std::collections::BTreeSet;

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{World, T};

/// One runtime application. Authored curves remain in its winning IMAD record.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSpaceLayer {
    pub modifier: GlobalFormId,
    pub strength: f32,
    pub age: f64,
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
