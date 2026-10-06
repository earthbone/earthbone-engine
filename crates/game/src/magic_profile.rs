// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-35 magic tick phases use PRF-12's bounded frame history and exclusive
//! clocks. The separate history reuses its fixed slots for phase statistics.

use soe_trace::{FrameHistory, FrameSample, FrameSnapshot, System, SystemStats};
use std::fmt::Write as _;

/// Work charged to the magic system during a Game tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicPhase {
    ImageSpace,
    Translations,
    ShaderParticles,
    EffectShaders,
    VisualEffects,
    ActiveEffects,
    Concentration,
    CastingAndDispatch,
}

impl MagicPhase {
    /// Every tick phase in execution order.
    pub const ALL: [Self; 8] = [
        Self::ImageSpace,
        Self::Translations,
        Self::ShaderParticles,
        Self::EffectShaders,
        Self::VisualEffects,
        Self::ActiveEffects,
        Self::Concentration,
        Self::CastingAndDispatch,
    ];

    /// Stable name for the PRF-12 report.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ImageSpace => "image_space",
            Self::Translations => "translations",
            Self::ShaderParticles => "shader_particles",
            Self::EffectShaders => "effect_shaders",
            Self::VisualEffects => "visual_effects",
            Self::ActiveEffects => "active_effects",
            Self::Concentration => "concentration",
            Self::CastingAndDispatch => "casting_and_dispatch",
        }
    }

    const fn slot(self) -> System {
        match self {
            Self::ImageSpace => System::Papyrus,
            Self::Translations => System::AiPlanning,
            Self::ShaderParticles => System::AiMovement,
            Self::EffectShaders => System::Combat,
            Self::VisualEffects => System::Magic,
            Self::ActiveEffects => System::Quests,
            Self::Concentration => System::Dialogue,
            Self::CastingAndDispatch => System::Physics,
        }
    }
}

#[derive(Default)]
pub(crate) struct MagicFrame(FrameSample);

impl MagicFrame {
    pub(crate) fn finish(&mut self, total_ns: u64) {
        self.0.system_ns[MagicPhase::CastingAndDispatch.slot() as usize] =
            total_ns.saturating_sub(self.0.frame_ns);
        self.0.frame_ns = total_ns;
    }
    pub(crate) fn record(&mut self, phase: MagicPhase, snapshot: Option<FrameSnapshot>) {
        if let Some(snapshot) = snapshot {
            let elapsed = snapshot.finish().system_ns[System::Magic as usize];
            self.0.system_ns[phase.slot() as usize] = elapsed;
            self.0.frame_ns = self.0.frame_ns.saturating_add(elapsed);
        }
    }
}

/// Rolling magic tick phase statistics, independent of presentation frames.
#[derive(Default)]
pub struct MagicFrameHistory(FrameHistory);

impl MagicFrameHistory {
    pub(crate) fn record(&mut self, frame: MagicFrame) {
        self.0.record(frame.0);
    }

    /// Nanoseconds charged to this phase in the latest Game tick.
    pub fn last_ns(&self, phase: MagicPhase) -> u64 {
        self.0.last().system_ns[phase.slot() as usize]
    }

    /// Rolling p50, p99 and worst over the latest 240 Game ticks.
    pub fn statistics(&self, phase: MagicPhase) -> SystemStats {
        self.0.statistics(phase.slot())
    }

    /// Phase totals and rolling statistics for the PRF-12 exit report.
    pub fn summary_text(&self) -> String {
        let mut text = String::from("Magic tick phases, included in magic system totals\n");
        let summary = self.0.summary_text();
        for (phase, line) in MagicPhase::ALL.into_iter().zip(summary.lines().skip(2)) {
            if let Some((_, values)) = line.split_once(' ') {
                let _ = writeln!(text, "magic.{} {values}", phase.name());
            }
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_35_phase_history_keeps_rolling_percentiles_and_idle_samples() {
        let mut history = MagicFrameHistory::default();
        for time in 1..=240 {
            let mut frame = MagicFrame::default();
            frame.0.system_ns[MagicPhase::ActiveEffects.slot() as usize] = time;
            history.record(frame);
        }
        let stats = history.statistics(MagicPhase::ActiveEffects);
        assert_eq!(
            (stats.p50_ns, stats.p99_ns, stats.worst_ns),
            (120, 238, 240)
        );
        assert_eq!(history.last_ns(MagicPhase::Translations), 0);
        for _ in 0..240 {
            history.record(MagicFrame::default());
        }
        assert_eq!(history.statistics(MagicPhase::ActiveEffects).worst_ns, 0);
        for phase in MagicPhase::ALL {
            assert!(history.summary_text().contains(phase.name()));
        }
    }

    #[test]
    fn prf_35_casting_remainder_accounts_for_the_complete_magic_slot() {
        let mut frame = MagicFrame::default();
        frame.0.system_ns[MagicPhase::ActiveEffects.slot() as usize] = 100;
        frame.0.frame_ns = 100;
        frame.finish(150);
        assert_eq!(frame.0.system_ns.iter().sum::<u64>(), 150);
        assert_eq!(
            frame.0.system_ns[MagicPhase::CastingAndDispatch.slot() as usize],
            50
        );
    }
}
