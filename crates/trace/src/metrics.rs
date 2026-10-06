// SPDX-License-Identifier: GPL-3.0-or-later

//! Always-on, low-cost counters and gauges for live runtime diagnostics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Number of stable runtime metrics in [`Metric`].
pub const METRIC_COUNT: usize = 27;

/// Stable diagnostic measurements shared by the runtime and Godot host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Metric {
    DetectionChecks,
    DetectionPairs,
    LineOfSightRaycasts,
    PathCacheHits,
    PathCacheMisses,
    PhysicsBodies,
    PhysicsContacts,
    AudioVoices,
    AudioDecodeNs,
    AudioFootstepLatencyNs,
    AudioVoiceLatencyNs,
    AudioSpellLatencyNs,
    AudioImpactLatencyNs,
    AudioOutputLatencyNs,
    AnimationActors,
    AnimationBehaviorNs,
    AnimationSamplingNs,
    AnimationBlendingNs,
    AnimationIkNs,
    AnimationSkinningNs,
    AnimationEventLatencyNs,
    AnimationGameTimeDriftNs,
    FrameTimeStddevNs,
    FrameTimeP50Ns,
    FrameTimeP99Ns,
    FrameTimeWorstNs,
    InputToScreenLatencyNs,
}

impl Metric {
    /// All metrics in their stable report order.
    pub const ALL: [Self; METRIC_COUNT] = [
        Self::DetectionChecks,
        Self::DetectionPairs,
        Self::LineOfSightRaycasts,
        Self::PathCacheHits,
        Self::PathCacheMisses,
        Self::PhysicsBodies,
        Self::PhysicsContacts,
        Self::AudioVoices,
        Self::AudioDecodeNs,
        Self::AudioFootstepLatencyNs,
        Self::AudioVoiceLatencyNs,
        Self::AudioSpellLatencyNs,
        Self::AudioImpactLatencyNs,
        Self::AudioOutputLatencyNs,
        Self::AnimationActors,
        Self::AnimationBehaviorNs,
        Self::AnimationSamplingNs,
        Self::AnimationBlendingNs,
        Self::AnimationIkNs,
        Self::AnimationSkinningNs,
        Self::AnimationEventLatencyNs,
        Self::AnimationGameTimeDriftNs,
        Self::FrameTimeStddevNs,
        Self::FrameTimeP50Ns,
        Self::FrameTimeP99Ns,
        Self::FrameTimeWorstNs,
        Self::InputToScreenLatencyNs,
    ];

    /// Stable field name used by reports and custom monitors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DetectionChecks => "detection_checks",
            Self::DetectionPairs => "detection_pairs",
            Self::LineOfSightRaycasts => "line_of_sight_raycasts",
            Self::PathCacheHits => "path_cache_hits",
            Self::PathCacheMisses => "path_cache_misses",
            Self::PhysicsBodies => "physics_bodies",
            Self::PhysicsContacts => "physics_contacts",
            Self::AudioVoices => "audio_voices",
            Self::AudioDecodeNs => "audio_decode_ns",
            Self::AudioFootstepLatencyNs => "audio_footstep_latency_ns",
            Self::AudioVoiceLatencyNs => "audio_voice_latency_ns",
            Self::AudioSpellLatencyNs => "audio_spell_latency_ns",
            Self::AudioImpactLatencyNs => "audio_impact_latency_ns",
            Self::AudioOutputLatencyNs => "audio_output_latency_ns",
            Self::AnimationActors => "animation_actors",
            Self::AnimationBehaviorNs => "animation_behavior_ns",
            Self::AnimationSamplingNs => "animation_sampling_ns",
            Self::AnimationBlendingNs => "animation_blending_ns",
            Self::AnimationIkNs => "animation_ik_ns",
            Self::AnimationSkinningNs => "animation_skinning_ns",
            Self::AnimationEventLatencyNs => "animation_event_latency_ns",
            Self::AnimationGameTimeDriftNs => "animation_game_time_drift_ns",
            Self::FrameTimeStddevNs => "frame_time_stddev_ns",
            Self::FrameTimeP50Ns => "frame_time_p50_ns",
            Self::FrameTimeP99Ns => "frame_time_p99_ns",
            Self::FrameTimeWorstNs => "frame_time_worst_ns",
            Self::InputToScreenLatencyNs => "input_to_screen_latency_ns",
        }
    }
}

static VALUES: [AtomicU64; METRIC_COUNT] = [const { AtomicU64::new(0) }; METRIC_COUNT];

/// Add to a cumulative counter or latency total.
pub fn metric_add(metric: Metric, value: u64) {
    VALUES[metric as usize].fetch_add(value, Ordering::Relaxed);
}

/// Replace a gauge or latest-sample value.
pub fn metric_set(metric: Metric, value: u64) {
    VALUES[metric as usize].store(value, Ordering::Relaxed);
}

/// Snapshot all measurements in [`Metric::ALL`] order.
#[must_use]
pub fn metric_snapshot() -> [u64; METRIC_COUNT] {
    std::array::from_fn(|index| VALUES[index].load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_24_metrics_have_unique_stable_names_and_round_trip_values() {
        assert_eq!(Metric::ALL.len(), METRIC_COUNT);
        for (index, metric) in Metric::ALL.iter().enumerate() {
            assert!(!metric.name().is_empty());
            assert!(!Metric::ALL[..index]
                .iter()
                .any(|other| other.name() == metric.name()));
        }
        metric_set(Metric::FrameTimeP99Ns, 17);
        metric_add(Metric::FrameTimeP99Ns, 5);
        assert_eq!(metric_snapshot()[Metric::FrameTimeP99Ns as usize], 22);
        metric_set(Metric::FrameTimeP99Ns, 0);
    }
}
