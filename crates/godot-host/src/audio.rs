// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning a decoded [`Pcm`] into the shape Godot's audio classes want, and
//! deciding which voice slot a sound gets.
//!
//! No Godot dependency lives here. `godot_audio` next door owns the nodes and
//! calls into this for every decision, the same split `skinned` and
//! `godot_actor` already use.
//!
//! The conversion exists because the two sides disagree about samples.
//! `soe_audio::Pcm` is interleaved 32 bit float, which is what every decoder
//! in that crate produces. `AudioStreamWAV` reads 8 bit, 16 bit, IMA ADPCM or
//! QOA and has no float format at all, so 16 bit is the widest lossless-enough
//! target and the one vanilla audio was in before it was compressed anyway.

use soe_audio::Pcm;

/// Largest magnitude a 16 bit sample can hold without wrapping to negative.
const I16_PEAK: f32 = 32767.0;

/// Distance-based player fallback until animation foot events drive playback.
#[derive(Debug, Default)]
pub struct FootstepCadence {
    distance: f32,
    right: bool,
}

impl FootstepCadence {
    pub fn reset(&mut self) {
        self.distance = 0.0;
        self.right = false;
    }

    /// Returns which foot landed. Airborne movement, invalid inputs and large
    /// discontinuities reset cadence without emitting an event.
    pub fn advance(&mut self, distance: f32, grounded: bool, stride: f32) -> Option<bool> {
        if !grounded
            || !distance.is_finite()
            || !stride.is_finite()
            || stride <= 0.0
            || distance < 0.0
            || distance > stride * 2.0
        {
            self.reset();
            return None;
        }
        self.distance += distance;
        if self.distance < stride {
            return None;
        }
        self.distance %= stride;
        let right = self.right;
        self.right = !self.right;
        Some(right)
    }
}

/// Interleaved float samples as little-endian 16 bit, the payload
/// `AudioStreamWAV` expects under `FORMAT_16_BITS`.
///
/// Samples outside the nominal range are clamped rather than allowed to wrap,
/// since a decoder that overshoots by a hair should not produce a click that
/// sounds like a different sample entirely.
#[must_use]
pub fn to_i16_bytes(pcm: &Pcm) -> Vec<u8> {
    let mut out = Vec::with_capacity(pcm.samples.len() * 2);
    for &s in &pcm.samples {
        let scaled = (s.clamp(-1.0, 1.0) * I16_PEAK).round() as i16;
        out.extend_from_slice(&scaled.to_le_bytes());
    }
    out
}

/// Averages every channel into one, which is what a positioned sound wants.
///
/// A stereo buffer played through a 3D player keeps its own left and right
/// image and fights the position the engine is trying to give it, so anything
/// placed in the world is folded down first. A buffer that is already mono is
/// returned unchanged.
#[must_use]
pub fn to_mono(pcm: &Pcm) -> Pcm {
    if pcm.channels <= 1 {
        return pcm.clone();
    }
    let channels = pcm.channels as usize;
    let frames = pcm.frames();
    let mut samples = Vec::with_capacity(frames);
    for f in 0..frames {
        let base = f * channels;
        let sum: f32 = pcm.samples[base..base + channels].iter().sum();
        samples.push(sum / channels as f32);
    }
    Pcm {
        sample_rate: pcm.sample_rate,
        channels: 1,
        samples,
    }
}

/// What a sound is for, which decides what gets sacrificed when the pool is
/// full. Ordered least to most important, so the derived `Ord` is the
/// priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Footsteps, impacts, the world making noise.
    Effect,
    /// A spoken line. Cutting one mid-word is far more noticeable than
    /// dropping a footstep, so voice outranks effects.
    Voice,
}

/// One playing sound's place in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playing {
    pub kind: Kind,
    /// Monotonic counter, not a clock, so the pool orders by age with no
    /// dependency on wall time and stays deterministic under test.
    pub started: u64,
}

/// A fixed set of playback slots.
///
/// Fixed because the alternative is one node per sound with nothing bounding
/// the count, and a crowded market would then create hundreds. When every slot
/// is busy the pool gives up the least important, oldest sound rather than
/// refusing to play the new one, which is the behaviour that keeps a footstep
/// from silencing dialogue.
#[derive(Debug)]
pub struct VoicePool {
    slots: Vec<Option<Playing>>,
    tick: u64,
}

impl VoicePool {
    /// A pool of `size` slots, all free.
    #[must_use]
    pub fn new(size: usize) -> Self {
        Self {
            slots: vec![None; size],
            tick: 0,
        }
    }

    /// How many slots exist in total.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// How many are playing right now.
    #[must_use]
    pub fn busy(&self) -> usize {
        self.slots
            .iter()
            .fold(0, |count, slot| count + usize::from(slot.is_some()))
    }

    /// Picks the slot a new sound of `kind` should use, or `None` when every
    /// slot holds something at least as important.
    ///
    /// A free slot wins. Otherwise the oldest sound of the lowest kind still
    /// below `kind` is given up. A sound never evicts one of its own kind or
    /// higher, so a stream of footsteps cannot chew through dialogue and the
    /// pool degrades by dropping the new sound instead.
    pub fn claim(&mut self, kind: Kind) -> Option<usize> {
        self.tick += 1;
        if let Some(i) = self.slots.iter().position(|s| s.is_none()) {
            self.slots[i] = Some(Playing {
                kind,
                started: self.tick,
            });
            return Some(i);
        }
        let victim = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.map(|p| (i, p)))
            .filter(|(_, p)| p.kind < kind)
            .min_by_key(|(_, p)| (p.kind, p.started))
            .map(|(i, _)| i)?;
        self.slots[victim] = Some(Playing {
            kind,
            started: self.tick,
        });
        Some(victim)
    }

    /// Marks a slot free again, which the host calls when a player reports it
    /// finished. Out of range and already free are both no-ops.
    pub fn release(&mut self, slot: usize) {
        if let Some(s) = self.slots.get_mut(slot) {
            *s = None;
        }
    }

    /// What occupies a slot, for tests and diagnostics.
    #[must_use]
    pub fn peek(&self, slot: usize) -> Option<Playing> {
        self.slots.get(slot).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::{to_i16_bytes, to_mono, Kind, VoicePool};
    use soe_audio::Pcm;

    #[test]
    fn footstep_cadence_uses_ground_distance_and_resets_on_jumps_or_warps() {
        let mut cadence = super::FootstepCadence::default();
        assert_eq!(cadence.advance(50.0, true, 90.0), None);
        for _ in 0..30 {
            assert_eq!(cadence.advance(0.0, true, 90.0), None);
        }
        assert_eq!(cadence.advance(40.0, true, 90.0), Some(false));
        assert_eq!(cadence.advance(90.0, true, 90.0), Some(true));
        assert_eq!(cadence.advance(100.0, false, 90.0), None);
        assert_eq!(cadence.advance(1000.0, true, 90.0), None);
        assert_eq!(cadence.advance(89.0, true, 90.0), None);
        assert_eq!(cadence.advance(1.0, true, 90.0), Some(false));
    }

    #[test]
    fn footstep_cadence_does_not_depend_on_frame_subdivision() {
        for count in [30, 60, 120] {
            let mut cadence = super::FootstepCadence::default();
            let feet: Vec<_> = (0..count)
                .filter_map(|_| cadence.advance(360.0 / count as f32, true, 90.0))
                .collect();
            assert_eq!(feet, [false, true, false, true]);
        }
    }

    fn pcm(channels: u16, samples: Vec<f32>) -> Pcm {
        Pcm {
            sample_rate: 44100,
            channels,
            samples,
        }
    }

    #[test]
    fn full_scale_samples_reach_the_ends_of_the_16_bit_range() {
        let bytes = to_i16_bytes(&pcm(1, vec![1.0, -1.0, 0.0]));
        assert_eq!(bytes.len(), 6);
        assert_eq!(i16::from_le_bytes([bytes[0], bytes[1]]), 32767);
        assert_eq!(i16::from_le_bytes([bytes[2], bytes[3]]), -32767);
        assert_eq!(i16::from_le_bytes([bytes[4], bytes[5]]), 0);
    }

    #[test]
    fn an_overshooting_decoder_clamps_instead_of_wrapping() {
        // Without the clamp this lands near -32768 and a loud sample would
        // sound like a click of the opposite sign.
        let bytes = to_i16_bytes(&pcm(1, vec![1.5, -1.5]));
        assert_eq!(i16::from_le_bytes([bytes[0], bytes[1]]), 32767);
        assert_eq!(i16::from_le_bytes([bytes[2], bytes[3]]), -32767);
    }

    #[test]
    fn mono_downmix_averages_channels_and_keeps_the_frame_count() {
        let stereo = pcm(2, vec![1.0, 0.0, 0.5, 0.5, -1.0, 1.0]);
        let mono = to_mono(&stereo);
        assert_eq!(mono.channels, 1);
        assert_eq!(mono.frames(), stereo.frames());
        assert_eq!(mono.samples, vec![0.5, 0.5, 0.0]);
        assert_eq!(mono.sample_rate, stereo.sample_rate);
    }

    #[test]
    fn a_mono_buffer_survives_the_downmix_untouched() {
        let one = pcm(1, vec![0.25, -0.25]);
        assert_eq!(to_mono(&one), one);
    }

    #[test]
    fn a_free_slot_is_preferred_over_evicting_anything() {
        let mut pool = VoicePool::new(2);
        assert_eq!(pool.claim(Kind::Effect), Some(0));
        assert_eq!(pool.claim(Kind::Voice), Some(1));
        assert_eq!(pool.busy(), 2);
    }

    #[test]
    fn a_voice_line_takes_the_oldest_effect_when_the_pool_is_full() {
        let mut pool = VoicePool::new(2);
        pool.claim(Kind::Effect);
        pool.claim(Kind::Effect);
        // Slot 0 is the older of the two effects, so it is the one given up.
        assert_eq!(pool.claim(Kind::Voice), Some(0));
        assert_eq!(pool.peek(0).map(|p| p.kind), Some(Kind::Voice));
        assert_eq!(pool.peek(1).map(|p| p.kind), Some(Kind::Effect));
    }

    #[test]
    fn a_footstep_never_cuts_a_voice_line_off() {
        let mut pool = VoicePool::new(1);
        pool.claim(Kind::Voice);
        // The new effect is dropped rather than silencing dialogue.
        assert_eq!(pool.claim(Kind::Effect), None);
        assert_eq!(pool.peek(0).map(|p| p.kind), Some(Kind::Voice));
    }

    #[test]
    fn a_voice_line_does_not_evict_another_voice_line() {
        let mut pool = VoicePool::new(1);
        pool.claim(Kind::Voice);
        assert_eq!(pool.claim(Kind::Voice), None);
    }

    #[test]
    fn releasing_a_slot_makes_it_available_again() {
        let mut pool = VoicePool::new(1);
        pool.claim(Kind::Voice);
        pool.release(0);
        assert_eq!(pool.busy(), 0);
        assert_eq!(pool.claim(Kind::Effect), Some(0));
        // Out of range is a no-op rather than a panic.
        pool.release(99);
    }
}
