// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-37 fixed simulation thread and immutable presentation handoff.
//! Godot's local thread_safe_apis.rst forbids active scene-tree access from
//! workers. Game and its Rc native closures are constructed and dropped on
//! one worker. PRF-17 retains ownership of navigation jobs. Rate and handoff
//! policy are engine-owned, rather than a vanilla save format.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use soe_esm::GlobalFormId;

use crate::world::RefPlacement;
use crate::Game;

const COMMAND_CAPACITY: usize = 64;

/// Independent simulation, AI and Papyrus rates. No frame count drives them.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub simulation_hz: u32,
    pub ai_hz: u32,
    pub scripts_hz: u32,
    /// CK fUpdateBudgetMS defaults to 1.2 ms, as recorded in docs/threading.md.
    pub papyrus_budget: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            simulation_hz: 60,
            ai_hz: 20,
            scripts_hz: 20,
            papyrus_budget: Duration::from_micros(1200),
        }
    }
}

impl Config {
    fn validate(self) -> Result<Self, Error> {
        if self.simulation_hz == 0
            || self.simulation_hz > 1000
            || self.ai_hz == 0
            || self.scripts_hz == 0
            || self.ai_hz > self.simulation_hz
            || self.scripts_hz > self.simulation_hz
            || self.papyrus_budget > Duration::from_secs(1)
        {
            return Err(Error::Configuration);
        }
        Ok(self)
    }

    /// Wall-clock interval between fixed simulation ticks.
    #[must_use]
    pub fn interval(self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.simulation_hz.max(1)))
    }
}

/// Failure to start a simulation. Initialization failures are published separately.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid simulation rates or Papyrus budget")]
    Configuration,
    #[error("could not spawn simulation thread: {0}")]
    Spawn(#[from] std::io::Error),
}

/// Latest host-controlled pose. The worker applies it to the authoritative World.
#[derive(Debug, Clone, Copy)]
pub struct PlayerPose {
    pub cell: Option<GlobalFormId>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub sprinting: bool,
}

impl PlayerPose {
    fn finite(self) -> bool {
        self.position
            .into_iter()
            .chain(self.rotation)
            .all(f32::is_finite)
    }
}

/// A resolved NPC capsule pose returned to its authoritative simulation cell.
#[derive(Debug, Clone, Copy)]
pub struct ActorPose {
    pub actor: GlobalFormId,
    pub cell: GlobalFormId,
    /// Snapshot position before the host resolves collision.
    pub source_position: [f32; 3],
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Default)]
struct HostPoses {
    player: Option<PlayerPose>,
    actors: BTreeMap<GlobalFormId, ActorPose>,
}

impl HostPoses {
    fn apply_actors(self, world: &mut crate::World) {
        for pose in self.actors.into_values() {
            if world.actor_ref(pose.actor).is_some_and(|actor| {
                actor
                    .ragdoll_forced
                    .unwrap_or(actor.dead || actor.unconscious)
            }) || world.grabbed_reference() == Some(pose.actor)
            {
                continue;
            }
            if world.reference_ref(pose.actor).is_some_and(|reference| {
                reference.cell == Some(pose.cell)
                    && reference.position == pose.source_position
                    && reference.rotation == pose.rotation
                    && reference.is_actor
                    && reference.enabled
                    && !reference.deleted
            }) {
                world.set_host_actor_pose(pose.actor, pose.position, pose.rotation);
            }
        }
    }
}

/// Ordered, bounded host commands. A full queue rejects a command for retry.
#[derive(Debug, Clone, Copy)]
pub enum Command {
    MenuMode(bool),
    Activate(GlobalFormId),
}

/// Read-only presentation values. These never become a second game-state store.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub sequence: u64,
    pub seconds: f64,
    pub produced_at: Instant,
    pub interval: Duration,
    pub behind_for: Duration,
    pub player: GlobalFormId,
    pub player_movement: crate::movement::Profile,
    pub actors: BTreeMap<GlobalFormId, RefPlacement>,
    pub contacts: Arc<Vec<crate::world::ContactActor>>,
    pub appearances: Arc<BTreeMap<GlobalFormId, crate::appearance::ActorAppearance>>,
    /// Append-only graph requests retain ordering when presentation skips ticks.
    pub animation_requests: Arc<Vec<crate::world::AnimationRequest>>,
    /// Ordered actor sounds retain simulation-owned variation draws.
    pub actor_sound_requests: Arc<Vec<crate::world::ActorSoundRequest>>,
    /// Attached actors whose warning graph has not completed.
    pub pending_warnings: Arc<BTreeSet<GlobalFormId>>,
    pub sky: Option<crate::world::sky::SkyFrame>,
    pub evaluations: usize,
    pub systems: soe_trace::FrameSample,
    /// PRF-17 summed search-worker busy time, included in tick wall time.
    pub worker_time: Duration,
}

/// Consecutive simulation snapshots, even when presentation skipped many ticks.
#[derive(Debug, Clone)]
pub struct SnapshotPair {
    pub previous: Arc<Snapshot>,
    pub current: Arc<Snapshot>,
}

impl SnapshotPair {
    /// One tick of interpolation delay. Missing actors and visibility changes
    /// use the current value. Rotations take the shortest route across wrap.
    #[must_use]
    pub fn placement(&self, actor: GlobalFormId, now: Instant) -> Option<RefPlacement> {
        let current = *self.current.actors.get(&actor)?;
        let Some(previous) = self.previous.actors.get(&actor) else {
            return Some(current);
        };
        if current.visible != previous.visible {
            return Some(current);
        }
        let fraction = (now
            .saturating_duration_since(self.current.produced_at)
            .as_secs_f64()
            / self.current.interval.as_secs_f64())
        .clamp(0.0, 1.0) as f32;
        Some(RefPlacement {
            position: std::array::from_fn(|axis| {
                previous.position[axis]
                    + (current.position[axis] - previous.position[axis]) * fraction
            }),
            rotation: std::array::from_fn(|axis| {
                let change = (current.rotation[axis] - previous.rotation[axis]
                    + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                previous.rotation[axis] + change * fraction
            }),
            scale: previous.scale + (current.scale - previous.scale) * fraction,
            visible: current.visible,
        })
    }

    /// Continuous overdue time, including a worker stalled since publication.
    #[must_use]
    pub fn lag(&self, now: Instant) -> Duration {
        self.current.behind_for
            + now
                .saturating_duration_since(self.current.produced_at)
                .saturating_sub(self.current.interval)
    }
}

/// Two immutable buffers exchanged in constant time. Neither side waits for
/// the other, and a retained reader buffer cannot be overwritten by a writer.
#[derive(Default)]
pub struct Snapshots {
    ready: Mutex<Option<Arc<SnapshotPair>>>,
}

impl Snapshots {
    /// Poll the newest pair. Contention retains the reader's previous pair.
    #[must_use]
    pub fn poll(&self) -> Option<Arc<SnapshotPair>> {
        self.ready.try_lock().ok()?.clone()
    }

    fn publish(&self, pair: Arc<SnapshotPair>) {
        if let Ok(mut ready) = self.ready.try_lock() {
            let old = ready.replace(pair);
            drop(ready);
            drop(old);
        }
    }
}

/// A simulation owner. Shutdown joins the worker, while frame methods never join.
pub struct Simulation {
    snapshots: Arc<Snapshots>,
    pose: Arc<Mutex<HostPoses>>,
    commands: SyncSender<Command>,
    stop: Arc<AtomicBool>,
    failure: Receiver<String>,
    worker: Option<JoinHandle<()>>,
}

impl Simulation {
    /// Construct Game inside a dedicated thread. The factory may capture shared
    /// immutable records and VFS assets, but no Godot objects or existing Game.
    ///
    /// # Errors
    /// Invalid rates or OS thread creation failure. Factory errors arrive through
    /// `failure`, without blocking the caller while initialization runs.
    pub fn spawn(
        config: Config,
        create: impl FnOnce() -> Result<Game, String> + Send + 'static,
    ) -> Result<Self, Error> {
        let config = config.validate()?;
        let snapshots = Arc::new(Snapshots::default());
        let pose = Arc::new(Mutex::new(HostPoses::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (failed, failure) = mpsc::sync_channel(1);
        let worker_snapshots = Arc::clone(&snapshots);
        let worker_pose = Arc::clone(&pose);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("soe-simulation".into())
            .spawn(move || {
                let run =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match create() {
                        Ok(game) => run(
                            game,
                            config,
                            &receiver,
                            &worker_pose,
                            &worker_snapshots,
                            &worker_stop,
                        ),
                        Err(error) => {
                            let _ = failed.try_send(error);
                        }
                    }));
                if run.is_err() {
                    let _ = failed.try_send("simulation thread panicked".into());
                }
                worker_stop.store(true, Ordering::Release);
            })?;
        Ok(Self {
            snapshots,
            pose,
            commands,
            stop,
            failure,
            worker: Some(worker),
        })
    }

    /// Poll without acquiring a World lock or waiting for the next simulation tick.
    #[must_use]
    pub fn poll(&self) -> Option<Arc<SnapshotPair>> {
        self.snapshots.poll()
    }

    /// Initialization or execution failure, consumed once.
    #[must_use]
    pub fn failure(&self) -> Option<String> {
        self.failure.try_recv().ok()
    }

    /// Coalesce host poses. False means invalid data or brief mailbox contention.
    pub fn set_player_pose(&self, pose: PlayerPose) -> bool {
        if !pose.finite() || self.stop.load(Ordering::Acquire) {
            return false;
        }
        let Ok(mut latest) = self.pose.try_lock() else {
            return false;
        };
        latest.player = Some(pose);
        true
    }

    /// Coalesce NPC physics feedback by FormID without waiting on the worker.
    /// Returns false for invalid data, shutdown or mailbox contention.
    /// The worker rejects a pose if its reference has departed the source cell.
    pub fn set_actor_pose(&self, pose: ActorPose) -> bool {
        if !pose
            .position
            .into_iter()
            .chain(pose.source_position)
            .chain(pose.rotation)
            .all(f32::is_finite)
            || self.stop.load(Ordering::Acquire)
        {
            return false;
        }
        let Ok(mut latest) = self.pose.try_lock() else {
            return false;
        };
        latest.actors.insert(pose.actor, pose);
        true
    }

    /// Queue once in submission order. False means full or disconnected.
    pub fn command(&self, command: Command) -> bool {
        self.commands.try_send(command).is_ok()
    }
}

impl Drop for Simulation {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

struct Cadence {
    elapsed: f64,
    phase: f64,
    period: f64,
}

impl Cadence {
    fn new(hz: u32) -> Self {
        Self {
            elapsed: 0.0,
            phase: 0.0,
            period: 1.0 / f64::from(hz),
        }
    }

    fn advance(&mut self, dt: f64) -> Option<f64> {
        self.elapsed += dt;
        self.phase += dt;
        if self.phase + 1e-9 < self.period {
            return None;
        }
        self.phase = (self.phase - self.period).max(0.0);
        Some(std::mem::take(&mut self.elapsed))
    }
}

fn run(
    mut game: Game,
    config: Config,
    commands: &Receiver<Command>,
    pose: &Mutex<HostPoses>,
    snapshots: &Snapshots,
    stop: &AtomicBool,
) {
    let interval = config.interval();
    let dt = 1.0 / f64::from(config.simulation_hz);
    let mut next = Instant::now();
    let mut behind_since = None;
    let mut ai = Cadence::new(config.ai_hz);
    let mut scripts = Cadence::new(config.scripts_hz);
    let mut menu_mode = false;
    let mut previous = None;
    let mut sequence = 0;
    let mut contacts = Arc::new(Vec::new());
    let mut sky = None;
    let mut appearances = Arc::new(BTreeMap::new());
    let mut animation_requests = Arc::new(Vec::new());
    let mut actor_sound_requests = Arc::new(Vec::new());
    let mut pending_warnings = Arc::new(BTreeSet::new());
    let player = game.player();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if now < next {
            thread::park_timeout(next.duration_since(now));
            continue;
        }
        let frame = soe_trace::FrameSnapshot::now();
        let scope = soe_trace::SystemScope::new(soe_trace::System::Other);
        let script_budget = if scripts.advance(dt).is_some() {
            config.papyrus_budget
        } else {
            Duration::ZERO
        };
        let mut ai_seconds = None;
        game.with_papyrus_budget(script_budget, |game| {
            for command in commands.try_iter().take(COMMAND_CAPACITY) {
                match command {
                    Command::MenuMode(paused) => menu_mode = paused,
                    Command::Activate(target) => {
                        let player = game.player();
                        game.activate(target, player);
                    }
                }
            }
            ai_seconds = if menu_mode {
                ai.elapsed = 0.0;
                ai.phase = 0.0;
                None
            } else {
                ai.advance(dt)
            };
            let latest = pose
                .try_lock()
                .ok()
                .map(|mut latest| std::mem::take(&mut *latest));
            if let Some(pose) = latest.as_ref().and_then(|latest| latest.player) {
                let player = game.player();
                let from = game
                    .world()
                    .reference_ref(player)
                    .map_or(pose.position, |r| r.position);
                if game.world().loaded_cell() == pose.cell {
                    if let Some(reference) = game.world_mut().reference(player) {
                        reference.position = pose.position;
                        reference.rotation = pose.rotation;
                    }
                    if let Some(actor) = game.world_mut().actor(player) {
                        actor.sprinting = pose.sprinting;
                    }
                    game.update_trigger_actor_position(player, pose.cell, from, pose.position);
                }
            }
            if let Some(latest) = latest {
                latest.apply_actors(&mut game.world_mut());
            }
            game.tick_scheduled(dt, menu_mode, ai_seconds, script_budget);
        });
        if ai_seconds.is_some() || sequence == 0 {
            let player = game.player();
            let mut world = game.world_mut();
            contacts = Arc::new(world.contact_actors());
            appearances = Arc::new(contacts.iter().filter(|contact| contact.actor != player)
                .filter_map(|contact| match world.actor_appearance(contact.actor) {
                    Ok(appearance) => Some((contact.actor, appearance)),
                    Err(error) => {
                        soe_trace::error!(target: "simulation::appearance", system = "host::actor", "{}: {error}", contact.actor);
                        None
                    }
                }).collect());
            sky = world
                .loaded_cell()
                .and_then(|cell| world.sky_frame(cell).ok().flatten());
        }
        sequence += 1;
        let player_movement = game.world_mut().actor_movement(player);
        let world = game.world();
        let actors = contacts
            .iter()
            .filter_map(|actor| {
                let mut placement = world.ref_placement(actor.actor)?;
                placement.visible &= !world.is_invisible(actor.actor);
                Some((actor.actor, placement))
            })
            .collect();
        let workers = if ai_seconds.is_none() || world.ai.planning_workers().jobs == 0 {
            Duration::ZERO
        } else {
            world.ai.planning_workers().busy.iter().copied().sum()
        };
        let seconds = world.clock.real_seconds;
        let evaluations = world.ai.evaluations;
        if animation_requests.len() != world.animation_requests.len() {
            animation_requests = Arc::new(world.animation_requests.clone());
        }
        if actor_sound_requests.len() != world.actor_sound_requests.len() {
            actor_sound_requests = Arc::new(world.actor_sound_requests.clone());
        }
        let warnings: BTreeSet<_> = contacts
            .iter()
            .filter_map(|contact| {
                world
                    .actor_ref(contact.actor)
                    .and_then(|actor| actor.aggro_warning)
                    .is_some_and(|warning| !warning.completed)
                    .then_some(contact.actor)
            })
            .collect();
        if warnings != *pending_warnings {
            pending_warnings = Arc::new(warnings);
        }
        drop(world);
        drop(scope);
        let produced_at = Instant::now();
        let due = next + interval;
        let behind_for = if produced_at > due {
            produced_at.saturating_duration_since(*behind_since.get_or_insert(due))
        } else {
            behind_since = None;
            Duration::ZERO
        };
        let current = Arc::new(Snapshot {
            sequence,
            seconds,
            produced_at,
            interval,
            behind_for,
            player,
            player_movement,
            actors,
            contacts: Arc::clone(&contacts),
            appearances: Arc::clone(&appearances),
            animation_requests: Arc::clone(&animation_requests),
            actor_sound_requests: Arc::clone(&actor_sound_requests),
            pending_warnings: Arc::clone(&pending_warnings),
            sky: sky.clone(),
            evaluations,
            systems: frame.finish(),
            worker_time: workers,
        });
        let pair = SnapshotPair {
            previous: previous.take().unwrap_or_else(|| Arc::clone(&current)),
            current: Arc::clone(&current),
        };
        snapshots.publish(Arc::new(pair));
        previous = Some(current);
        next += interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> GlobalFormId {
        GlobalFormId {
            file: 0,
            object: 0x800,
        }
    }

    fn snapshot(sequence: u64, x: f32, now: Instant) -> Arc<Snapshot> {
        Arc::new(Snapshot {
            sequence,
            seconds: sequence as f64 / 60.0,
            produced_at: now,
            interval: Duration::from_millis(20),
            behind_for: Duration::ZERO,
            player: GlobalFormId {
                file: 0,
                object: 0x14,
            },
            player_movement: crate::movement::Profile::default(),
            actors: BTreeMap::from([(
                id(),
                RefPlacement {
                    position: [x, 0.0, 0.0],
                    rotation: [0.0; 3],
                    scale: 1.0,
                    visible: true,
                },
            )]),
            contacts: Arc::new(Vec::new()),
            appearances: Arc::new(BTreeMap::new()),
            animation_requests: Arc::new(Vec::new()),
            actor_sound_requests: Arc::new(Vec::new()),
            pending_warnings: Arc::new(BTreeSet::new()),
            sky: None,
            evaluations: 0,
            systems: soe_trace::FrameSample::default(),
            worker_time: Duration::ZERO,
        })
    }

    #[test]
    fn prf_37_double_buffer_never_overwrites_a_retained_reader() {
        let now = Instant::now();
        let snapshots = Snapshots::default();
        snapshots.publish(Arc::new(SnapshotPair {
            previous: snapshot(1, 1.0, now),
            current: snapshot(2, 2.0, now),
        }));
        let retained = snapshots.poll().unwrap();
        snapshots.publish(Arc::new(SnapshotPair {
            previous: snapshot(8, 8.0, now),
            current: snapshot(9, 9.0, now),
        }));
        assert_eq!(retained.current.actors[&id()].position[0], 2.0);
        let latest = snapshots.poll().unwrap();
        assert_eq!((latest.previous.sequence, latest.current.sequence), (8, 9));
    }

    #[test]
    fn prf_37_contended_handoff_neither_waits_nor_replaces_a_pair() {
        let now = Instant::now();
        let snapshots = Snapshots::default();
        let pair = Arc::new(SnapshotPair {
            previous: snapshot(1, 1.0, now),
            current: snapshot(2, 2.0, now),
        });
        snapshots.publish(Arc::clone(&pair));
        let held = snapshots.ready.lock().unwrap();
        assert!(snapshots.poll().is_none());
        snapshots.publish(Arc::new(SnapshotPair {
            previous: snapshot(3, 3.0, now),
            current: snapshot(4, 4.0, now),
        }));
        drop(held);
        assert!(Arc::ptr_eq(&snapshots.poll().unwrap(), &pair));
    }

    #[test]
    fn prf_37_interpolation_clamps_and_wraps_angles_without_extrapolation() {
        let now = Instant::now();
        let mut previous = snapshot(1, 0.0, now);
        let mut current = snapshot(2, 10.0, now);
        Arc::get_mut(&mut previous)
            .unwrap()
            .actors
            .get_mut(&id())
            .unwrap()
            .rotation[2] = 3.1;
        Arc::get_mut(&mut current)
            .unwrap()
            .actors
            .get_mut(&id())
            .unwrap()
            .rotation[2] = -3.1;
        let pair = SnapshotPair { previous, current };
        let halfway = pair
            .placement(id(), now + Duration::from_millis(10))
            .unwrap();
        assert_eq!(halfway.position[0], 5.0);
        assert!((halfway.rotation[2] - std::f32::consts::PI).abs() < 0.0001);
        assert_eq!(
            pair.placement(id(), now + Duration::from_secs(2))
                .unwrap()
                .position[0],
            10.0
        );
    }

    #[test]
    fn prf_37_stalled_snapshot_reports_more_than_one_second_behind() {
        let now = Instant::now();
        let pair = SnapshotPair {
            previous: snapshot(1, 0.0, now),
            current: snapshot(2, 1.0, now),
        };
        assert!(pair.lag(now + Duration::from_millis(1021)) > Duration::from_secs(1));
    }

    #[test]
    fn prf_37_ai_and_scripts_keep_configured_rates_and_accumulated_time() {
        for hz in [1, 15, 20, 25, 60] {
            let mut cadence = Cadence::new(hz);
            let updates: Vec<_> = (0..60)
                .filter_map(|_| cadence.advance(1.0 / 60.0))
                .collect();
            assert_eq!(updates.len(), hz as usize);
            assert!((updates.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn prf_37_commands_are_bounded_and_invalid_configuration_is_rejected() {
        let (release, wait) = mpsc::channel();
        let simulation = Simulation::spawn(Config::default(), move || {
            wait.recv().unwrap();
            Ok(Game::new(soe_esm::LoadOrder::new()))
        })
        .unwrap();
        for _ in 0..COMMAND_CAPACITY {
            assert!(simulation.command(Command::MenuMode(true)));
        }
        assert!(!simulation.command(Command::MenuMode(false)));
        assert!(!simulation.set_player_pose(PlayerPose {
            cell: None,
            position: [f32::NAN; 3],
            rotation: [0.0; 3],
            sprinting: false
        }));
        release.send(()).unwrap();
        drop(simulation);
        assert!(matches!(
            Simulation::spawn(
                Config {
                    simulation_hz: 0,
                    ..Config::default()
                },
                || unreachable!()
            ),
            Err(Error::Configuration)
        ));
    }

    #[test]
    fn npc_physics_mailbox_coalesces_poses_and_rejects_invalid_or_contended_input() {
        let (release, wait) = mpsc::channel();
        let simulation = Simulation::spawn(Config::default(), move || {
            wait.recv().unwrap();
            Ok(Game::new(soe_esm::LoadOrder::new()))
        })
        .unwrap();
        let mut pose = ActorPose {
            actor: id(),
            cell: GlobalFormId {
                file: 0,
                object: 0x801,
            },
            source_position: [0.0; 3],
            position: [10.0, 0.0, 0.0],
            rotation: [0.0; 3],
        };
        assert!(simulation.set_actor_pose(pose));
        pose.position[0] = 20.0;
        assert!(simulation.set_actor_pose(pose));
        let held = simulation.pose.lock().unwrap();
        assert_eq!(held.actors.len(), 1);
        assert_eq!(held.actors[&id()].position, pose.position);
        assert!(
            !simulation.set_actor_pose(pose),
            "submission must not wait on the worker"
        );
        drop(held);
        pose.position[2] = f32::NAN;
        assert!(!simulation.set_actor_pose(pose));
        assert_eq!(
            simulation.pose.lock().unwrap().actors[&id()].position,
            [20.0, 0.0, 0.0]
        );
        release.send(()).unwrap();
        drop(simulation);
    }

    #[test]
    fn npc_physics_feedback_updates_world_and_rejects_departed_or_disabled_actors() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        plugin.push_top(crate::npc::build_record(
            0x803,
            "GroundedActor",
            "Grounded Actor",
            crate::npc::Config::default(),
            crate::npc::Stats::default(),
        ));
        plugin.push_top(
            soe_esm::Record::new(soe_esm::Tag(*b"ACHR"), id().object)
                .sub(soe_esm::Tag(*b"NAME"), 0x803u32.to_le_bytes())
                .sub(soe_esm::Tag(*b"DATA"), [0; 24]),
        );
        let mut order = soe_esm::LoadOrder::new();
        order
            .add(
                "NpcPose.esm",
                soe_esm::Plugin::parse(&plugin.to_bytes()).unwrap(),
            )
            .unwrap();
        let game = Game::new(order);
        let cell = GlobalFormId {
            file: 0,
            object: 0x801,
        };
        game.world_mut().reference(id()).unwrap().cell = Some(cell);
        let pose = ActorPose {
            actor: id(),
            cell,
            source_position: [0.0; 3],
            position: [20.0, 0.0, 0.0],
            rotation: [0.0; 3],
        };
        let apply = |world: &mut crate::World, pose| {
            HostPoses {
                player: None,
                actors: BTreeMap::from([(id(), pose)]),
            }
            .apply_actors(world);
        };
        apply(&mut game.world_mut(), pose);
        assert_eq!(
            game.world().reference_ref(id()).unwrap().position,
            pose.position
        );
        assert_eq!(
            game.world().reference_ref(id()).unwrap().rotation,
            pose.rotation
        );
        game.world_mut().reference(id()).unwrap().rotation[2] = 1.0;
        let old_rotation = ActorPose {
            source_position: pose.position,
            ..pose
        };
        apply(&mut game.world_mut(), old_rotation);
        assert_eq!(game.world().reference_ref(id()).unwrap().rotation[2], 1.0);
        game.world_mut().reference(id()).unwrap().rotation = [0.0; 3];
        game.world_mut().reference(id()).unwrap().position = [30.0, 0.0, 0.0];
        apply(&mut game.world_mut(), pose);
        assert_eq!(
            game.world().reference_ref(id()).unwrap().position,
            [30.0, 0.0, 0.0]
        );
        game.world_mut().reference(id()).unwrap().cell = Some(GlobalFormId {
            file: 0,
            object: 0x802,
        });
        let stale = ActorPose {
            source_position: [30.0, 0.0, 0.0],
            position: [40.0, 0.0, 0.0],
            ..pose
        };
        apply(&mut game.world_mut(), stale);
        assert_eq!(
            game.world().reference_ref(id()).unwrap().position,
            [30.0, 0.0, 0.0]
        );
        {
            let mut world = game.world_mut();
            let reference = world.reference(id()).unwrap();
            reference.cell = Some(cell);
            reference.enabled = false;
        }
        apply(&mut game.world_mut(), stale);
        assert_eq!(
            game.world().reference_ref(id()).unwrap().position,
            [30.0, 0.0, 0.0]
        );
    }
}
