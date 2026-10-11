// SPDX-License-Identifier: GPL-3.0-or-later

use soe_trace::{FrameHistory, FrameSample, System, FRAME_HISTORY};

#[test]
fn prf_12_rolling_window_wrap_keeps_idle_systems_and_nearest_rank_values() {
    let mut history = FrameHistory::default();
    let total_frames = FRAME_HISTORY + 2;

    for frame in 0..total_frames {
        let mut sample = FrameSample {
            frame_ns: 100_000_000,
            ..FrameSample::default()
        };
        sample.system_ns[System::Papyrus as usize] = frame as u64 * 10;
        history.record(sample);
    }

    let stats = history.statistics(System::Papyrus);
    assert_eq!(history.frames(), total_frames as u64);
    assert_eq!(
        history.last().system_ns[System::Papyrus as usize],
        (total_frames - 1) as u64 * 10
    );
    assert_eq!(stats.p50_ns, 1210);
    assert_eq!(stats.p99_ns, 2390);
    assert_eq!(stats.worst_ns, 2410);

    for system in System::ALL
        .into_iter()
        .filter(|system| *system != System::Papyrus)
    {
        assert_eq!(
            history.last().system_ns[system as usize],
            0,
            "{}",
            system.name()
        );
        assert_eq!(history.statistics(system).worst_ns, 0, "{}", system.name());
    }
}
