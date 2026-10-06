// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{metric_add, metric_set, metric_snapshot, Metric, METRIC_COUNT};

#[test]
fn prf_24_metrics_keep_the_report_schema_and_counter_gauge_semantics() {
    let names: Vec<_> = Metric::ALL.iter().map(|metric| metric.name()).collect();
    assert_eq!(names.len(), METRIC_COUNT);
    assert_eq!(
        names.iter().collect::<std::collections::HashSet<_>>().len(),
        METRIC_COUNT
    );
    assert_eq!(
        names,
        [
            "detection_checks",
            "detection_pairs",
            "line_of_sight_raycasts",
            "path_cache_hits",
            "path_cache_misses",
            "physics_bodies",
            "physics_contacts",
            "audio_voices",
            "audio_decode_ns",
            "audio_footstep_latency_ns",
            "audio_voice_latency_ns",
            "audio_spell_latency_ns",
            "audio_impact_latency_ns",
            "audio_output_latency_ns",
            "animation_actors",
            "animation_behavior_ns",
            "animation_sampling_ns",
            "animation_blending_ns",
            "animation_ik_ns",
            "animation_skinning_ns",
            "animation_event_latency_ns",
            "animation_game_time_drift_ns",
            "frame_time_stddev_ns",
            "frame_time_p50_ns",
            "frame_time_p99_ns",
            "frame_time_worst_ns",
            "input_to_screen_latency_ns",
        ]
    );

    let counter_before = metric_snapshot()[Metric::DetectionChecks as usize];
    metric_add(Metric::DetectionChecks, 2);
    assert_eq!(
        metric_snapshot()[Metric::DetectionChecks as usize],
        counter_before + 2
    );

    metric_set(Metric::AudioVoices, 7);
    assert_eq!(metric_snapshot()[Metric::AudioVoices as usize], 7);
    metric_set(Metric::AudioVoices, 3);
    assert_eq!(metric_snapshot()[Metric::AudioVoices as usize], 3);
    metric_set(Metric::AudioVoices, 0);
}
