// SPDX-License-Identifier: GPL-3.0-or-later

//! Distance-based driver cadence with actor-owned saved clocks.
//! CommonLibSSE-NG AIProcess.h and
//! ProcessLists.h supply the four levels, not distances or update intervals.
//! OpenMW mwmechanics/actors.cpp also gates actor work by player distance.
//! The intervals here are engine policy. They do not replace actor state.

use std::collections::HashMap;

use soe_esm::GlobalFormId;

use super::{distance, Procedure, World};

/// Driver update detail relative to the player, in decreasing frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessLevel {
    /// Nearby actors and actors in combat or controlled by a scene.
    High,
    /// Actors within two exterior cell widths of the player.
    MiddleHigh,
    /// Actors within four exterior cell widths of the player.
    MiddleLow,
    /// Distant actors or actors in another interior or worldspace.
    Low,
}

impl ProcessLevel {
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(super) fn interval(self) -> f64 {
        match self {
            Self::High => 0.0,
            Self::MiddleHigh => 0.1,
            Self::MiddleLow => 0.5,
            Self::Low => 5.0,
        }
    }
}

/// Accumulated simulation time and time to the actor's next coarse update.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessClock {
    pub(crate) elapsed: f64,
    pub(crate) until: f64,
}

impl ProcessClock {
    pub(crate) fn advance(
        state: &mut Option<Self>,
        actor: GlobalFormId,
        level: ProcessLevel,
        dt: f64,
        urgent: bool,
    ) -> Option<f64> {
        let interval = level.interval();
        let clock = state.get_or_insert_with(|| Self {
            // CONTEXT: Spread first updates across the interval without random state.
            until: interval * f64::from(actor.object % 300) / 300.0,
            ..Self::default()
        });
        clock.elapsed += dt;
        clock.until = clock.until.min(interval);
        clock.until -= dt;
        if interval == 0.0 || urgent || clock.until <= 1e-9 {
            let elapsed = std::mem::take(&mut clock.elapsed);
            clock.until = interval;
            Some(elapsed)
        } else {
            None
        }
    }
}

/// Scheduling inputs derived from the live player and cell records.
pub(crate) struct PlayerContext {
    pub(crate) position: [f32; 3],
    pub(crate) cell: Option<GlobalFormId>,
    pub(crate) space: Option<GlobalFormId>,
}

pub(crate) struct Schedule {
    pub(crate) actors: Vec<GlobalFormId>,
    pub(crate) updates: Vec<(GlobalFormId, f64)>,
    pub(crate) inactive: Vec<GlobalFormId>,
    pub(crate) counts: [usize; 4],
    pub(crate) update_counts: [usize; 4],
}

pub(crate) fn coordinate_space(world: &World, cell: Option<GlobalFormId>) -> Option<GlobalFormId> {
    let cell = cell?;
    if world.cell_is_interior(cell) {
        return None;
    }
    world.resolve_from(cell, world.record(cell)?.placement.world?)
}

pub(crate) fn level_from_state(
    state: &crate::world::ActorState,
    reference: Option<&crate::world::RefState>,
    is_player: bool,
    player: Option<&PlayerContext>,
    spaces: &HashMap<GlobalFormId, Option<GlobalFormId>>,
) -> ProcessLevel {
    let Some(player) = player else {
        return ProcessLevel::High;
    };
    if is_player
        || state.in_combat
        || state.teammate
        || state.ai.scene_package.is_some()
        || matches!(
            state.ai.procedure,
            Procedure::ForceGreet | Procedure::DialogueActivate
        )
    {
        return ProcessLevel::High;
    }
    let Some(reference) = reference else {
        return ProcessLevel::Low;
    };
    if reference.cell != player.cell {
        let actor_space = reference
            .cell
            .and_then(|cell| spaces.get(&cell).copied().flatten());
        if player.space.is_none() || player.space != actor_space {
            return ProcessLevel::Low;
        }
    }
    let separation = distance(player.position, reference.position);
    if separation <= 4096.0 {
        ProcessLevel::High
    } else if separation <= 8192.0 {
        ProcessLevel::MiddleHigh
    } else if separation <= 16384.0 {
        ProcessLevel::MiddleLow
    } else {
        ProcessLevel::Low
    }
}

#[cfg(test)]
pub(super) fn level(
    world: &World,
    actor: GlobalFormId,
    player: Option<([f32; 3], Option<GlobalFormId>)>,
    spaces: &mut HashMap<GlobalFormId, Option<GlobalFormId>>,
) -> ProcessLevel {
    let player = player.map(|(position, cell)| PlayerContext {
        position,
        cell,
        space: coordinate_space(world, cell),
    });
    let Some(state) = world.actor_ref(actor) else {
        return ProcessLevel::Low;
    };
    let reference = world.reference_ref(actor);
    if let Some(cell) = reference.and_then(|reference| reference.cell) {
        spaces
            .entry(cell)
            .or_insert_with(|| coordinate_space(world, Some(cell)));
    }
    level_from_state(
        state,
        reference,
        world.is_player(actor),
        player.as_ref(),
        spaces,
    )
}

pub(crate) fn write_clock(
    actor: &crate::world::ActorState,
    writer: &mut soe_papyrus::vm::state::Writer,
    _pool: &mut soe_papyrus::vm::state::ArrayPool,
) {
    if let Some(clock) = &actor.process_clock {
        writer.f64(clock.elapsed);
        writer.f64(clock.until);
    }
}

pub(crate) fn read_clock(
    actor: &mut crate::world::ActorState,
    reader: &mut crate::world::save::FormReader<'_, '_>,
    _table: &soe_papyrus::vm::state::ArrayTable,
) -> Result<(), soe_papyrus::VmError> {
    let elapsed = reader.f64()?;
    let until = reader.f64()?;
    if !elapsed.is_finite()
        || !until.is_finite()
        || !(0.0..=5.0).contains(&elapsed)
        || !(0.0..=5.0).contains(&until)
    {
        return Err(soe_papyrus::VmError::State("invalid process clock".into()));
    }
    actor.process_clock = Some(ProcessClock { elapsed, until });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
        for object in [0x14, 0x801] {
            plugin.push_top(
                Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()),
            );
        }
        plugin.push_top(Record::new(Tag(*b"CELL"), 0x900).sub(Tag(*b"DATA"), [1, 0]));
        plugin.push_top(Record::new(Tag(*b"CELL"), 0x901).sub(Tag(*b"DATA"), [1, 0]));
        for (cell, worldspace) in [(0x902, 0xA00), (0x903, 0xA00), (0x904, 0xA01)] {
            let mut record = Record::new(Tag(*b"CELL"), cell).sub(Tag(*b"DATA"), [0, 0]);
            record.placement.world = Some(worldspace);
            plugin.push_top(record);
        }
        let mut order = LoadOrder::new();
        order.add("Prf36.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.player();
        world.actor(id(0x14)).unwrap();
        world.actor(id(0x801)).unwrap();
        world.reference(id(0x801)).unwrap().cell = Some(id(0x900));
        world
    }

    #[test]
    fn prf_36_distance_bands_and_unrelated_interiors_select_process_level() {
        let mut world = world();
        let player = Some(([0.0; 3], Some(id(0x900))));
        let mut spaces = HashMap::new();
        for (distance, expected) in [
            (4096.0, ProcessLevel::High),
            (4097.0, ProcessLevel::MiddleHigh),
            (8193.0, ProcessLevel::MiddleLow),
            (16385.0, ProcessLevel::Low),
        ] {
            world.reference(id(0x801)).unwrap().position[0] = distance;
            assert_eq!(level(&world, id(0x801), player, &mut spaces), expected);
        }
        world.reference(id(0x801)).unwrap().position = [0.0; 3];
        world.reference(id(0x801)).unwrap().cell = Some(id(0x901));
        assert_eq!(
            level(&world, id(0x801), player, &mut spaces),
            ProcessLevel::Low
        );
        world.actor(id(0x801)).unwrap().in_combat = true;
        assert_eq!(
            level(&world, id(0x801), player, &mut spaces),
            ProcessLevel::High
        );
    }

    #[test]
    fn prf_36_coarse_ticks_preserve_elapsed_motion_and_stagger_first_updates() {
        let mut clocks: HashMap<GlobalFormId, Option<ProcessClock>> = HashMap::new();
        let mut totals = [0.0; 300];
        let mut counts = [0; 300];
        let mut first_updates = vec![0; 300];
        for first in &mut first_updates {
            for actor in 0..300 {
                if let Some(dt) = ProcessClock::advance(
                    clocks.entry(id(actor)).or_default(),
                    id(actor),
                    ProcessLevel::Low,
                    1.0 / 60.0,
                    false,
                ) {
                    totals[actor as usize] += dt;
                    counts[actor as usize] += 1;
                    if counts[actor as usize] == 1 {
                        *first += 1;
                    }
                }
            }
        }
        assert!(first_updates.iter().all(|count| *count <= 2));
        assert!(counts.iter().all(|count| *count >= 1));
        for actor in 0..300 {
            let remainder = clocks[&id(actor)].as_ref().unwrap().elapsed;
            assert!((totals[actor as usize] + remainder - 5.0).abs() < 1e-8);
        }
    }

    #[test]
    fn prf_36_reevaluation_and_promotion_do_not_wait_for_coarse_tick() {
        let mut clock = None;
        let actor = id(299);
        assert!(ProcessClock::advance(&mut clock, actor, ProcessLevel::Low, 0.1, false).is_none());
        assert_eq!(
            ProcessClock::advance(&mut clock, actor, ProcessLevel::Low, 0.1, true),
            Some(0.2)
        );
        assert_eq!(
            ProcessClock::advance(&mut clock, actor, ProcessLevel::High, 0.1, false),
            Some(0.1)
        );
    }

    #[test]
    fn prf_36_far_actor_moves_on_coarse_tick_and_explicit_request_runs_next_tick() {
        let mut world = world();
        world.reference(id(0x14)).unwrap().cell = Some(id(0x900));
        world.reference(id(0x801)).unwrap().cell = Some(id(0x901));
        world.actor(id(0x801)).unwrap().ai = super::super::AiState {
            procedure: Procedure::Travel,
            speed: 100.0,
            destination: Some([10000.0, 0.0, 0.0]),
            path: vec![[10000.0, 0.0, 0.0]],
            ..super::super::AiState::default()
        };
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        for _ in 0..10 {
            world.clock.real_seconds += 0.1;
            super::super::tick(&mut world, 0.1, &mut alloc);
        }
        assert_eq!(world.reference_ref(id(0x801)).unwrap().position, [0.0; 3]);
        world.actor(id(0x801)).unwrap().ai.reevaluate = true;
        world.clock.real_seconds += 0.1;
        super::super::tick(&mut world, 0.1, &mut alloc);
        assert!((world.reference_ref(id(0x801)).unwrap().position[0] - 110.0).abs() < 0.001);
        assert_eq!(world.ai.process_updates[ProcessLevel::Low.index()], 1);
    }

    #[test]
    fn prf_36_no_package_retries_at_one_second_instead_of_every_frame() {
        let mut world = world();
        world.reference(id(0x14)).unwrap().cell = Some(id(0x900));
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        for _ in 0..30 {
            world.clock.real_seconds += 1.0 / 60.0;
            super::super::tick(&mut world, 1.0 / 60.0, &mut alloc);
        }
        assert_eq!(world.ai.evaluations, 1);
        world.actor(id(0x801)).unwrap().ai.reevaluate = true;
        world.clock.real_seconds += 1.0 / 60.0;
        super::super::tick(&mut world, 1.0 / 60.0, &mut alloc);
        assert_eq!(world.ai.evaluations, 2);
    }

    #[test]
    fn prf_36_exterior_distance_compares_cells_only_in_the_same_worldspace() {
        let mut world = world();
        let player = Some(([0.0; 3], Some(id(0x902))));
        world.reference(id(0x801)).unwrap().cell = Some(id(0x903));
        assert_eq!(
            level(&world, id(0x801), player, &mut HashMap::new()),
            ProcessLevel::High
        );
        world.reference(id(0x801)).unwrap().cell = Some(id(0x904));
        assert_eq!(
            level(&world, id(0x801), player, &mut HashMap::new()),
            ProcessLevel::Low
        );
    }

    #[test]
    fn prf_36_saved_coarse_clock_keeps_next_update_and_missing_section_defaults() {
        let mut original = world();
        original.reference(id(0x14)).unwrap().cell = Some(id(0x900));
        original.reference(id(0x801)).unwrap().cell = Some(id(0x901));
        original.actor(id(0x801)).unwrap().ai = super::super::AiState {
            procedure: Procedure::Travel,
            speed: 100.0,
            destination: Some([10000.0, 0.0, 0.0]),
            path: vec![[10000.0, 0.0, 0.0]],
            ..super::super::AiState::default()
        };
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        for _ in 0..10 {
            original.clock.real_seconds += 0.1;
            super::super::tick(&mut original, 0.1, &mut alloc);
        }
        let bytes = original.save_state();
        let mut restored = world();
        restored.load_state(&bytes).unwrap();
        for _ in 0..50 {
            for world in [&mut original, &mut restored] {
                world.clock.real_seconds += 0.1;
                super::super::tick(world, 0.1, &mut alloc);
            }
            assert_eq!(
                original.reference_ref(id(0x801)).unwrap().position,
                restored.reference_ref(id(0x801)).unwrap().position
            );
            assert_eq!(
                original.actor_ref(id(0x801)).unwrap().process_clock,
                restored.actor_ref(id(0x801)).unwrap().process_clock
            );
        }
        original.actor(id(0x801)).unwrap().process_clock = None;
        restored.load_state(&original.save_state()).unwrap();
        assert!(restored
            .actor_ref(id(0x801))
            .unwrap()
            .process_clock
            .is_none());
    }

    #[test]
    fn prf_36_invalid_saved_clock_names_actor_section_and_preserves_source() {
        let mut source = world();
        let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
        source.clock.real_seconds = 0.1;
        super::super::tick(&mut source, 0.1, &mut alloc);
        let before = source.save_state();
        let mut bad = before.clone();
        let name = b"PRF-36";
        let marker = bad
            .windows(name.len())
            .position(|bytes| bytes == name)
            .unwrap();
        let length_at = marker + name.len();
        let payload_at = length_at + std::mem::size_of::<u32>();
        let length = u32::from_le_bytes(bad[length_at..payload_at].try_into().unwrap());
        assert_eq!(length as usize, 2 * std::mem::size_of::<f64>());
        bad[payload_at..payload_at + std::mem::size_of::<f64>()]
            .copy_from_slice(&f64::NAN.to_le_bytes());
        let error = source.load_state(&bad).unwrap_err().to_string();
        assert!(error.contains("section PRF-36 at actor"), "{error}");
        assert_eq!(source.save_state(), before);
    }
}
