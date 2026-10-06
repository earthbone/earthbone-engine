// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient AI measurements over the existing actor driver. Level names follow
//! CommonLibSSE-NG AIProcess.h and ProcessLists.h. The current driver updates
//! every built NPC at high process, without a distance-based process scheduler.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;

use super::{distance, AiState, Procedure};

/// Procedure categories present even when idle in the host report.
pub const PROCEDURE_PHASES: [&str; 24] = [
    "procedure.None",
    "procedure.Travel",
    "procedure.Sandbox",
    "procedure.Sleep",
    "procedure.Flee",
    "procedure.Patrol",
    "procedure.Guard",
    "procedure.Sit",
    "procedure.UseMagic",
    "procedure.UseWeapon",
    "procedure.Wait",
    "procedure.Hover",
    "procedure.Follow",
    "procedure.Escort",
    "procedure.Orbit",
    "procedure.ForceGreet",
    "procedure.DialogueActivate",
    "procedure.UseIdleMarker",
    "procedure.HoldPosition",
    "procedure.Shout",
    "procedure.Unsupported",
    "procedure.Activate",
    "procedure.Find",
    "procedure.UseItemAt",
];

/// Permanent phase slots. Idle or unavailable work reports zero.
pub const DECISION_PHASES: [&str; 12] = [
    "package_selection",
    "schedule_evaluation",
    "combat_target",
    "combat_acquisition",
    "combat_style_choice",
    "spell_choice",
    "weapon_choice",
    "greeting_evaluation",
    "force_greet_evaluation",
    "idle_chatter_evaluation",
    "sandbox_markers",
    "steering",
];

pub(super) const PHASES: [&str; 53] = [
    "package_selection",
    "schedule_evaluation",
    "conditions",
    "package_switch",
    "marker_base",
    "marker_bed",
    "marker_reference",
    "idle_markers",
    "cell_references",
    "marker_resume",
    "actor_driver",
    "actor_state",
    "turn_undead_check",
    "calm_check",
    "evaluation_due",
    "flee_movement",
    "turn_undead_movement",
    "combat_flags",
    "combat_target",
    "combat_acquisition",
    "combat_movement",
    "movement",
    "procedure_tree",
    "weapon_choice",
    "spell_choice",
    "sandbox_markers",
    "path_requests",
    "steering",
    "procedure.None",
    "procedure.Travel",
    "procedure.Sandbox",
    "procedure.Sleep",
    "procedure.Flee",
    "procedure.Patrol",
    "procedure.Guard",
    "procedure.Sit",
    "procedure.UseMagic",
    "procedure.UseWeapon",
    "procedure.Wait",
    "procedure.Hover",
    "procedure.Follow",
    "procedure.Escort",
    "procedure.Orbit",
    "procedure.ForceGreet",
    "procedure.DialogueActivate",
    "procedure.UseIdleMarker",
    "procedure.HoldPosition",
    "procedure.Shout",
    "procedure.Unsupported",
    "procedure.Activate",
    "procedure.Find",
    "procedure.UseItemAt",
    "force_greet_evaluation",
];

pub(super) struct PhaseSlots {
    pub(super) times: [Duration; PHASES.len()],
    pub(super) counts: [u64; PHASES.len()],
}

impl Default for PhaseSlots {
    fn default() -> Self {
        Self {
            times: [Duration::ZERO; PHASES.len()],
            counts: [0; PHASES.len()],
        }
    }
}

/// Fixed accumulator slots, with names published outside actor processing.
#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum Phase {
    PackageSelection,
    ScheduleEvaluation,
    Conditions,
    PackageSwitch,
    MarkerBase,
    MarkerBed,
    MarkerReference,
    IdleMarkers,
    CellReferences,
    MarkerResume,
    ActorDriver,
    ActorState,
    TurnUndeadCheck,
    CalmCheck,
    EvaluationDue,
    FleeMovement,
    TurnUndeadMovement,
    CombatFlags,
    CombatTarget,
    CombatAcquisition,
    CombatMovement,
    Movement,
    ProcedureTree,
    WeaponChoice,
    SpellChoice,
    SandboxMarkers,
    PathRequests,
    Steering,
    None,
    Travel,
    Sandbox,
    Sleep,
    Flee,
    Patrol,
    Guard,
    Sit,
    UseMagic,
    UseWeapon,
    Wait,
    Hover,
    Follow,
    Escort,
    Orbit,
    ForceGreet,
    DialogueActivate,
    UseIdleMarker,
    HoldPosition,
    Shout,
    Unsupported,
    Activate,
    Find,
    UseItemAt,
    ForceGreetEvaluation,
}

impl Phase {
    pub(crate) const fn name(self) -> &'static str {
        PHASES[self as usize]
    }
}

/// Process slots in CommonLib's PROCESS_TYPE order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum ProcessLevel {
    High,
    MiddleHigh,
    MiddleLow,
    Low,
}

impl ProcessLevel {
    /// All process levels in source order.
    pub const ALL: [Self; 4] = [Self::High, Self::MiddleHigh, Self::MiddleLow, Self::Low];

    /// Identifier used by the host report.
    pub const fn name(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::MiddleHigh => "middle_high",
            Self::MiddleLow => "middle_low",
            Self::Low => "low",
        }
    }
}

/// Last tick's actor count, updates and inclusive driver time.
#[derive(Debug, Clone, Default)]
pub struct ProcessMetrics {
    pub actors: u64,
    pub updates: u64,
    pub time: Duration,
    /// Mean seconds between updates of actors observed in consecutive ticks.
    pub mean_interval_seconds: f64,
}

/// Events whose outstanding latency is measured until an NPC response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Stimulus {
    DetectedPlayer,
    PlayerAttack,
    PlayerCrime,
    PlayerDialogue,
}

impl Stimulus {
    /// All observed stimulus kinds.
    pub const ALL: [Self; 4] = [
        Self::DetectedPlayer,
        Self::PlayerAttack,
        Self::PlayerCrime,
        Self::PlayerDialogue,
    ];

    /// Identifier used by the host report.
    pub const fn name(self) -> &'static str {
        match self {
            Self::DetectedPlayer => "detected_player",
            Self::PlayerAttack => "player_attack",
            Self::PlayerCrime => "player_crime",
            Self::PlayerDialogue => "player_dialogue",
        }
    }
}

/// Actual response boundaries, rather than queued work or eligibility checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    Alarm,
    Combat,
    Greeting,
}

/// Completed wall-clock reaction latencies for one actor and stimulus kind.
#[derive(Debug, Clone, Default)]
pub struct ReactionMetrics {
    pub responses: u64,
    pub total: Duration,
    pub last: Duration,
    pub worst: Duration,
    pub response: Option<Response>,
    /// Repeated stimuli retain the earliest outstanding timestamp.
    pub pending: Option<Instant>,
}

/// One completed or cancelled request, including all frame slices.
#[derive(Debug, Clone)]
pub struct PathSample {
    pub started: Instant,
    pub ready: Instant,
    pub expanded: u64,
    /// Pulled polyline length in game units, independent of A* traversal cost.
    pub length: f32,
    pub failed: bool,
    pub cancelled: bool,
    pub replan: bool,
}

/// Per-actor path totals and a bounded rolling window of completed requests.
#[derive(Debug, Default)]
pub struct PathMetrics {
    pub requests: u64,
    pub completed: u64,
    pub failures: u64,
    pub cancellations: u64,
    pub replans: u64,
    pub expanded: u64,
    pub samples: VecDeque<PathSample>,
    recent_requests: VecDeque<(Instant, bool)>,
}

impl PathMetrics {
    pub(super) fn started(&mut self, at: Instant) -> bool {
        let replan = self.requests > 0;
        self.requests += 1;
        self.replans += u64::from(replan);
        self.recent_requests.push_back((at, replan));
        while self
            .recent_requests
            .front()
            .is_some_and(|(started, _)| at.duration_since(*started).as_secs_f64() > 1.0)
            || self.recent_requests.len() > 4096
        {
            self.recent_requests.pop_front();
        }
        replan
    }

    pub(super) fn finished(&mut self, sample: PathSample) {
        self.completed += u64::from(!sample.cancelled);
        self.failures += u64::from(sample.failed && !sample.cancelled);
        self.cancellations += u64::from(sample.cancelled);
        self.expanded += sample.expanded;
        let ready = sample.ready;
        self.samples.push_back(sample);
        while self
            .samples
            .front()
            .is_some_and(|old| ready.duration_since(old.ready).as_secs_f64() > 1.0)
            || self.samples.len() > 4096
        {
            self.samples.pop_front();
        }
    }

    /// Requests and re-plans started in the preceding wall-clock second.
    /// At most 4096 recent requests per actor are retained.
    pub fn rates(&self, now: Instant) -> (usize, usize) {
        self.recent_requests
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at).as_secs_f64() <= 1.0)
            .fold((0, 0), |(requests, replans), (_, replan)| {
                (requests + 1, replans + usize::from(*replan))
            })
    }
}

/// Measurements of one live actor, separate from its saved AI state.
#[derive(Debug, Default)]
pub struct ActorMetrics {
    pub updates: u64,
    pub time: Duration,
    pub reactions: Box<[ReactionMetrics; 4]>,
    pub stuck: bool,
    last_update: Option<f64>,
    progress: Option<Progress>,
}

#[derive(Debug)]
struct Progress {
    destination: [f32; 3],
    remaining: f32,
    since: f64,
}

/// Always-on measurements. Entries disappear when their actor is unloaded.
#[derive(Default)]
pub struct Metrics {
    pub levels: [ProcessMetrics; 4],
    pub actors: BTreeMap<GlobalFormId, ActorMetrics>,
}

impl Metrics {
    pub(super) fn begin_frame(&mut self, actors: &[GlobalFormId]) {
        self.levels = std::array::from_fn(|_| ProcessMetrics::default());
        let mut index = 0;
        self.actors.retain(|actor, _| {
            while actors.get(index).is_some_and(|loaded| loaded < actor) {
                index += 1;
            }
            actors.get(index) == Some(actor)
        });
    }

    pub(super) fn actor_updated(
        &mut self,
        actor: GlobalFormId,
        now: f64,
        duration: Duration,
        progress: Option<([f32; 3], f32)>,
    ) {
        let metrics = self.actors.entry(actor).or_default();
        metrics.updates += 1;
        metrics.time += duration;
        let level = &mut self.levels[ProcessLevel::High as usize];
        level.actors += 1;
        level.updates += 1;
        level.time += duration;
        let interval = metrics
            .last_update
            .map_or(0.0, |previous| (now - previous).max(0.0));
        level.mean_interval_seconds +=
            (interval - level.mean_interval_seconds) / level.updates as f64;
        metrics.last_update = Some(now);
        Self::update_progress(metrics, now, progress);
    }

    #[cfg(test)]
    fn progress(
        &mut self,
        actor: GlobalFormId,
        now: f64,
        position: [f32; 3],
        ai: &AiState,
        active: bool,
    ) {
        let progress = Self::path_progress(position, ai, active);
        Self::update_progress(self.actors.entry(actor).or_default(), now, progress);
    }

    pub(super) fn path_progress(
        position: [f32; 3],
        ai: &AiState,
        active: bool,
    ) -> Option<([f32; 3], f32)> {
        let destination = ai
            .destination
            .filter(|_| active && !ai.arrived && !ai.path.is_empty())?;
        let mut previous = position;
        let remaining = ai
            .path
            .iter()
            .map(|point| {
                let length = distance(previous, *point);
                previous = *point;
                length
            })
            .sum::<f32>();
        remaining.is_finite().then_some((destination, remaining))
    }

    fn update_progress(metrics: &mut ActorMetrics, now: f64, progress: Option<([f32; 3], f32)>) {
        let Some((destination, remaining)) = progress.filter(|_| now.is_finite()) else {
            metrics.progress = None;
            metrics.stuck = false;
            return;
        };
        let progress = metrics.progress.get_or_insert(Progress {
            destination,
            remaining,
            since: now,
        });
        if progress.destination != destination
            || now < progress.since
            || remaining < progress.remaining - 0.01
        {
            *progress = Progress {
                destination,
                remaining,
                since: now,
            };
        }
        metrics.stuck = now - progress.since >= 5.0;
    }

    /// Start a latency only for an accepted stimulus. Repeats retain its origin.
    pub(crate) fn stimulus(&mut self, actor: GlobalFormId, stimulus: Stimulus) {
        self.stimulus_at(actor, stimulus, Instant::now());
    }

    pub(crate) fn stimulus_at(&mut self, actor: GlobalFormId, stimulus: Stimulus, at: Instant) {
        self.actors.entry(actor).or_default().reactions[stimulus as usize]
            .pending
            .get_or_insert(at);
    }

    /// Finish compatible outstanding stimuli at an actual alarm, combat or greeting.
    pub(crate) fn response(&mut self, actor: GlobalFormId, response: Response) {
        self.response_at(actor, response, Instant::now());
    }

    fn response_at(&mut self, actor: GlobalFormId, response: Response, at: Instant) {
        let Some(metrics) = self.actors.get_mut(&actor) else {
            return;
        };
        for stimulus in Stimulus::ALL {
            if (stimulus == Stimulus::PlayerDialogue && response != Response::Greeting)
                || (matches!(stimulus, Stimulus::PlayerAttack | Stimulus::PlayerCrime)
                    && response == Response::Greeting)
            {
                continue;
            }
            let metric = &mut metrics.reactions[stimulus as usize];
            if let Some(started) = metric.pending.take() {
                let duration = at.saturating_duration_since(started);
                metric.responses += 1;
                metric.total += duration;
                metric.last = duration;
                metric.worst = metric.worst.max(duration);
                metric.response = Some(response);
            }
        }
    }
}

impl Procedure {
    /// Stable procedure category, including the currently unsupported category.
    pub fn timing_phase(&self) -> &'static str {
        self.phase().name()
    }

    /// Stable procedure category, including the currently unsupported category.
    pub(super) fn phase(&self) -> Phase {
        match self {
            Self::None => Phase::None,
            Self::Travel => Phase::Travel,
            Self::Sandbox => Phase::Sandbox,
            Self::Sleep => Phase::Sleep,
            Self::Flee => Phase::Flee,
            Self::Patrol => Phase::Patrol,
            Self::Guard => Phase::Guard,
            Self::Sit => Phase::Sit,
            Self::UseMagic => Phase::UseMagic,
            Self::UseWeapon => Phase::UseWeapon,
            Self::Wait { .. } => Phase::Wait,
            Self::Hover => Phase::Hover,
            Self::Follow => Phase::Follow,
            Self::Escort => Phase::Escort,
            Self::Orbit { .. } => Phase::Orbit,
            Self::ForceGreet => Phase::ForceGreet,
            Self::DialogueActivate => Phase::DialogueActivate,
            Self::UseIdleMarker { .. } => Phase::UseIdleMarker,
            Self::HoldPosition => Phase::HoldPosition,
            Self::Shout => Phase::Shout,
            Self::Unsupported(name) if name.eq_ignore_ascii_case("UseItemAt") => Phase::UseItemAt,
            Self::Unsupported(_) => Phase::Unsupported,
            Self::Activate => Phase::Activate,
            Self::Find => Phase::Find,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> GlobalFormId {
        GlobalFormId {
            file: 0,
            object: 0x800,
        }
    }

    #[test]
    fn prf_27_stuck_requires_five_seconds_without_path_progress() {
        let mut metrics = Metrics::default();
        let mut ai = AiState {
            destination: Some([100.0, 0.0, 0.0]),
            path: vec![[100.0, 0.0, 0.0]],
            ..AiState::default()
        };
        for now in [0.0, 4.999] {
            metrics.progress(actor(), now, [0.0; 3], &ai, true);
            assert!(!metrics.actors[&actor()].stuck);
        }
        metrics.progress(actor(), 5.0, [0.0; 3], &ai, true);
        assert!(metrics.actors[&actor()].stuck);
        metrics.progress(actor(), 5.1, [1.0, 0.0, 0.0], &ai, true);
        assert!(!metrics.actors[&actor()].stuck);
        metrics.progress(actor(), 10.1, [0.0; 3], &ai, true);
        assert!(
            metrics.actors[&actor()].stuck,
            "backtracking is not progress"
        );
        ai.arrived = true;
        metrics.progress(actor(), 11.0, [0.0; 3], &ai, true);
        assert!(!metrics.actors[&actor()].stuck, "arrival clears the flag");
    }

    #[test]
    fn prf_27_reaction_latency_pairs_each_stimulus_with_actual_response() {
        let mut metrics = Metrics::default();
        let start = Instant::now();
        for stimulus in Stimulus::ALL {
            metrics.stimulus_at(actor(), stimulus, start);
            metrics.stimulus_at(actor(), stimulus, start + Duration::from_millis(10));
        }
        metrics.response_at(actor(), Response::Alarm, start + Duration::from_millis(20));
        for stimulus in [
            Stimulus::DetectedPlayer,
            Stimulus::PlayerAttack,
            Stimulus::PlayerCrime,
        ] {
            let metric = &metrics.actors[&actor()].reactions[stimulus as usize];
            assert_eq!(metric.responses, 1);
            assert_eq!(metric.last, Duration::from_millis(20));
            assert_eq!(metric.response, Some(Response::Alarm));
        }
        assert_eq!(
            metrics.actors[&actor()].reactions[Stimulus::PlayerDialogue as usize].responses,
            0
        );
        metrics.response_at(
            actor(),
            Response::Greeting,
            start + Duration::from_millis(30),
        );
        assert_eq!(
            metrics.actors[&actor()].reactions[Stimulus::PlayerDialogue as usize].last,
            Duration::from_millis(30)
        );
        metrics.stimulus_at(actor(), Stimulus::DetectedPlayer, start);
        metrics.response_at(actor(), Response::Combat, start + Duration::from_millis(40));
        assert_eq!(
            metrics.actors[&actor()].reactions[0].worst,
            Duration::from_millis(40)
        );
        assert_eq!(
            metrics.actors[&actor()].reactions[0].total,
            Duration::from_millis(60)
        );
        metrics.response_at(actor(), Response::Combat, start + Duration::from_millis(50));
        assert_eq!(
            metrics.actors[&actor()].reactions[0].responses,
            2,
            "an unmatched response is not a sample"
        );
    }

    #[test]
    fn prf_27_process_counts_timings_cadence_reset_and_unload() {
        let mut metrics = Metrics::default();
        metrics.begin_frame(&[actor()]);
        metrics.actor_updated(actor(), 1.0, Duration::from_micros(20), None);
        metrics.begin_frame(&[actor()]);
        metrics.actor_updated(actor(), 1.25, Duration::from_micros(30), None);
        let high = &metrics.levels[ProcessLevel::High as usize];
        assert_eq!(high.actors, 1);
        assert_eq!(high.updates, 1);
        assert_eq!(high.time, Duration::from_micros(30));
        assert_eq!(high.mean_interval_seconds, 0.25);
        assert_eq!(metrics.actors[&actor()].updates, 2);
        assert_eq!(metrics.actors[&actor()].time, Duration::from_micros(50));
        for level in [
            ProcessLevel::MiddleHigh,
            ProcessLevel::MiddleLow,
            ProcessLevel::Low,
        ] {
            assert_eq!(metrics.levels[level as usize].updates, 0);
            assert_eq!(metrics.levels[level as usize].time, Duration::ZERO);
        }
        metrics.begin_frame(&[]);
        assert!(metrics.actors.is_empty());
        assert_eq!(metrics.levels[0].actors, 0);
    }
    #[test]
    fn prf_27_every_procedure_has_a_distinct_permanent_timing_slot() {
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
        plugin
            .push_top(Record::new(Tag(*b"ACHR"), 0x800).sub(Tag(*b"NAME"), 0x801u32.to_le_bytes()));
        let mut order = LoadOrder::new();
        order.add("Prf27.esm", plugin).unwrap();
        let mut world = crate::World::new(order);
        let procedures = [
            Procedure::None,
            Procedure::Travel,
            Procedure::Sandbox,
            Procedure::Sleep,
            Procedure::Flee,
            Procedure::Patrol,
            Procedure::Guard,
            Procedure::Sit,
            Procedure::UseMagic,
            Procedure::UseWeapon,
            Procedure::Wait {
                stop_movement: true,
            },
            Procedure::Hover,
            Procedure::Follow,
            Procedure::Escort,
            Procedure::Orbit { height: 0.0 },
            Procedure::ForceGreet,
            Procedure::DialogueActivate,
            Procedure::UseIdleMarker { marker: None },
            Procedure::HoldPosition,
            Procedure::Shout,
            Procedure::Unsupported("unknown".into()),
            Procedure::Activate,
            Procedure::Find,
            Procedure::Unsupported("UseItemAt".into()),
        ];
        for procedure in procedures {
            let phase = procedure.timing_phase();
            world.actor(actor()).unwrap().ai = AiState {
                procedure,
                ..Default::default()
            };
            let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
            super::super::step(&mut world, actor(), 0.01, &mut alloc);
            assert_eq!(world.ai.phase_calls[phase], 1, "{phase}");
            assert!(world.ai.planning_phases[phase] > Duration::ZERO, "{phase}");
        }
        assert_eq!(
            PROCEDURE_PHASES.len(),
            world
                .ai
                .planning_phases
                .keys()
                .filter(|phase| phase.starts_with("procedure."))
                .count()
        );
    }
}
