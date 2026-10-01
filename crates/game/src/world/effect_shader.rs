// SPDX-License-Identifier: GPL-3.0-or-later

//! Runtime applications following shipped `EffectShader.psc` semantics. xEdit
//! `wbDefinitionsTES5.pas` identifies the referenced EFSH record type.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{World, T};

/// One shader playing on one placed reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectShaderPlayback {
    pub shader: GlobalFormId,
    pub target: GlobalFormId,
    /// Remaining real seconds, or `None` for an infinite playback.
    pub remaining: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EffectShaderState {
    playbacks: BTreeMap<(GlobalFormId, GlobalFormId), EffectShaderPlayback>,
}

impl World {
    /// Play a shader on a placed reference. A negative duration is infinite.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-EFSH form, a non-reference target or a
    /// non-finite duration. No playback changes on error.
    pub fn play_effect_shader(
        &mut self,
        shader: GlobalFormId,
        target: GlobalFormId,
        duration: f32,
    ) -> Result<(), VmError> {
        self.validate_effect_shader(shader)?;
        self.validate_effect_target(target)?;
        if !duration.is_finite() {
            return Err(VmError::State("non-finite effect shader duration".into()));
        }
        let playback = EffectShaderPlayback {
            shader,
            target,
            remaining: (duration >= 0.0).then_some(f64::from(duration)),
        };
        let key = (shader, target);
        if playback.remaining == Some(0.0) {
            self.effect_shaders.playbacks.remove(&key);
        } else {
            self.effect_shaders.playbacks.insert(key, playback);
        }
        self.effect_shader(shader, target, duration, playback.remaining != Some(0.0));
        Ok(())
    }

    /// Stop one shader on one reference. Other shader and target pairs remain.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-EFSH form or a non-reference target.
    pub fn stop_effect_shader(
        &mut self,
        shader: GlobalFormId,
        target: GlobalFormId,
    ) -> Result<(), VmError> {
        self.validate_effect_shader(shader)?;
        self.validate_effect_target(target)?;
        self.effect_shaders.playbacks.remove(&(shader, target));
        self.effect_shader(shader, target, 0.0, false);
        Ok(())
    }

    /// Active applications in stable shader and target order.
    #[must_use]
    pub fn effect_shader_playbacks(&self) -> Vec<EffectShaderPlayback> {
        self.effect_shaders.playbacks.values().copied().collect()
    }

    pub(crate) fn restore_effect_shaders(&mut self, state: EffectShaderState) {
        self.effect_shader_requests.clear();
        for playback in self.effect_shader_playbacks() {
            self.effect_shader(playback.shader, playback.target, 0.0, false);
        }
        self.effect_shaders = state;
        for playback in self.effect_shader_playbacks() {
            self.effect_shader(
                playback.shader,
                playback.target,
                playback
                    .remaining
                    .map_or(-1.0, |remaining| remaining as f32),
                true,
            );
        }
    }

    fn validate_effect_shader(&self, shader: GlobalFormId) -> Result<(), VmError> {
        if self.tag_of(shader) != Some(T(b"EFSH")) {
            return Err(VmError::State("form is not an EffectShader".into()));
        }
        Ok(())
    }

    fn validate_effect_target(&self, target: GlobalFormId) -> Result<(), VmError> {
        if !matches!(self.tag_of(target), Some(tag) if tag == T(b"REFR") || tag == T(b"ACHR")) {
            return Err(VmError::State(
                "effect shader target is not a reference".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn tick_effect_shaders(&mut self, seconds: f64) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        self.effect_shaders.playbacks.retain(|_, playback| {
            let Some(remaining) = &mut playback.remaining else {
                return true;
            };
            *remaining -= seconds;
            *remaining > 0.0
        });
    }

    pub(crate) fn write_effect_shaders(&self, writer: &mut Writer) {
        writer.len(self.effect_shaders.playbacks.len());
        for playback in self.effect_shaders.playbacks.values() {
            write_form_id(writer, playback.shader);
            write_form_id(writer, playback.target);
            writer.bool(playback.remaining.is_some());
            if let Some(remaining) = playback.remaining {
                writer.f64(remaining);
            }
        }
    }

    pub(crate) fn read_effect_shaders(
        &self,
        reader: &mut FormReader<'_, '_>,
    ) -> Result<EffectShaderState, VmError> {
        let mut state = EffectShaderState::default();
        for _ in 0..reader.len()? {
            let shader = read_form_id(reader)?;
            let target = read_form_id(reader)?;
            let remaining = if reader.bool()? {
                let value = reader.f64()?;
                if !value.is_finite() || value <= 0.0 {
                    return Err(VmError::State("invalid effect shader lifetime".into()));
                }
                Some(value)
            } else {
                None
            };
            self.validate_effect_shader(shader)?;
            self.validate_effect_target(target)?;
            let playback = EffectShaderPlayback {
                shader,
                target,
                remaining,
            };
            if state.playbacks.insert((shader, target), playback).is_some() {
                return Err(VmError::State("duplicate effect shader playback".into()));
            }
        }
        Ok(state)
    }

    pub(crate) fn read_effect_shader_section(
        &mut self,
        reader: &mut FormReader<'_, '_>,
    ) -> Result<(), VmError> {
        self.effect_shaders = self.read_effect_shaders(reader)?;
        Ok(())
    }

    pub(crate) fn write_effect_shader_section(&self, writer: &mut Writer) {
        self.write_effect_shaders(writer);
    }
}
