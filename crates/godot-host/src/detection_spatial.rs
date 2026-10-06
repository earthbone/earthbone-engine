// SPDX-License-Identifier: GPL-3.0-or-later

//! CPU detection neighborhoods bounded by the CK fSneakMaxDistance setting.
//! Sound sources are indexed separately from their owning actor, following
//! CK CreateDetectionEvent. Grid entries are rebuilt from current placements.

use std::collections::{BTreeMap, HashMap};

use soe_esm::GlobalFormId;
use soe_game::detection::{DetectionEvent, Motion, Observation};
use soe_game::world::ContactActor;

use super::{length, sub};

type Bucket = (GlobalFormId, [i32; 3]);

#[derive(Default)]
struct TargetSample<'a> {
    nearby: Option<&'a ContactActor>,
    sound: Option<(usize, &'a DetectionEvent)>,
}

pub(super) fn bucket(point: [f32; 3], width: f32) -> [i32; 3] {
    point.map(|value| (value / width).floor() as i32)
}

fn neighbors(center: [i32; 3]) -> impl Iterator<Item = [i32; 3]> {
    (-1_i32..=1).flat_map(move |x| {
        (-1_i32..=1).flat_map(move |y| {
            (-1_i32..=1).map(move |z| {
                [
                    center[0].saturating_add(x),
                    center[1].saturating_add(y),
                    center[2].saturating_add(z),
                ]
            })
        })
    })
}

/// A frame's detection range and CELL-to-worldspace coordinate identities.
/// Light sampling retains the original CELL, while pair grids share an exterior
/// world's coordinate frame, including its persistent-reference parent cell.
pub struct DetectionSpaces {
    range: f32,
    cells: HashMap<GlobalFormId, GlobalFormId>,
}

impl DetectionSpaces {
    fn separate_cells(range: f32) -> Self {
        Self {
            range,
            cells: HashMap::new(),
        }
    }

    /// Resolve coordinate identities once from the shared winning cell index.
    pub fn new(
        world: &soe_game::World,
        actors: &[ContactActor],
        events: &[DetectionEvent],
        range: f32,
    ) -> Self {
        let mut result = Self::separate_cells(range);
        for cell in actors
            .iter()
            .map(|actor| actor.cell)
            .chain(events.iter().map(|event| event.cell))
        {
            result
                .cells
                .entry(cell)
                .or_insert_with(|| world.cell_coordinate_space(cell));
        }
        result
    }

    fn space(&self, cell: GlobalFormId) -> GlobalFormId {
        self.cells.get(&cell).copied().unwrap_or(cell)
    }

    /// Keep living resident placements that can observe another actor or a sound
    /// in range. Isolated actors need no detection body or controller-height read.
    /// NPC pairs away from the player remain eligible, as do distant sound owners.
    pub fn candidate_actors(
        &self,
        actors: Vec<ContactActor>,
        events: &[DetectionEvent],
    ) -> Vec<ContactActor> {
        let range = self.range;
        if !range.is_finite() || range <= 0.0 {
            return Vec::new();
        }
        let mut grid: HashMap<Bucket, Vec<usize>> = HashMap::new();
        for (i, actor) in actors.iter().enumerate() {
            grid.entry((self.space(actor.cell), bucket(actor.position, range)))
                .or_default()
                .push(i);
        }
        let mut keep = vec![false; actors.len()];
        for (i, observer) in actors.iter().enumerate() {
            keep[i] = neighbors(bucket(observer.position, range)).any(|key| {
                grid.get(&(self.space(observer.cell), key))
                    .is_some_and(|indices| {
                        indices.iter().any(|&j| {
                            actors[j].actor != observer.actor
                                && length(sub(observer.position, actors[j].position)) <= range
                        })
                    })
            });
        }
        for event in events {
            if let Some(i) = actors.iter().position(|actor| actor.actor == event.owner) {
                keep[i] = true;
            }
            for key in neighbors(bucket(event.position, range)) {
                if let Some(indices) = grid.get(&(self.space(event.cell), key)) {
                    for &i in indices {
                        if actors[i].actor != event.owner
                            && length(sub(actors[i].position, event.position)) <= range
                        {
                            keep[i] = true;
                        }
                    }
                }
            }
        }
        actors
            .into_iter()
            .zip(keep)
            .filter_map(|(actor, keep)| keep.then_some(actor))
            .collect()
    }

    /// Ordered nearby actor pairs and sounds within detection range. Sight and
    /// light are sampled only for nearby actors, once per target for light.
    /// A nearby sound can still belong to a distant or nonresident actor.
    pub fn observations(
        &self,
        actors: &[ContactActor],
        events: &[DetectionEvent],
        motions: &BTreeMap<GlobalFormId, Motion>,
        mut visible: impl FnMut(&ContactActor, &ContactActor) -> bool,
        mut light: impl FnMut(GlobalFormId, [f32; 3]) -> f32,
    ) -> Vec<Observation> {
        let range = self.range;
        if !range.is_finite() || range <= 0.0 {
            return Vec::new();
        }
        let mut actor_grid: HashMap<Bucket, Vec<usize>> = HashMap::new();
        let mut event_grid: HashMap<Bucket, Vec<usize>> = HashMap::new();
        let mut actor_ids = BTreeMap::new();
        for (i, actor) in actors.iter().enumerate() {
            actor_grid
                .entry((self.space(actor.cell), bucket(actor.position, range)))
                .or_default()
                .push(i);
            actor_ids.insert(actor.actor, actor);
        }
        for (i, event) in events.iter().enumerate() {
            event_grid
                .entry((self.space(event.cell), bucket(event.position, range)))
                .or_default()
                .push(i);
        }
        let mut levels = BTreeMap::new();
        let mut out = Vec::new();
        for observer in actors {
            let mut targets: BTreeMap<GlobalFormId, TargetSample<'_>> = BTreeMap::new();
            for key in neighbors(bucket(observer.position, range)) {
                if let Some(indices) = actor_grid.get(&(self.space(observer.cell), key)) {
                    for &i in indices {
                        let target = &actors[i];
                        if target.actor != observer.actor
                            && length(sub(observer.position, target.position)) <= range
                        {
                            targets.entry(target.actor).or_default().nearby = Some(target);
                        }
                    }
                }
                if let Some(indices) = event_grid.get(&(self.space(observer.cell), key)) {
                    for &i in indices {
                        let event = &events[i];
                        if event.owner == observer.actor
                            || length(sub(observer.position, event.position)) > range
                        {
                            continue;
                        }
                        let sound = &mut targets.entry(event.owner).or_default().sound;
                        if sound.is_none_or(|(previous_index, previous)| {
                            event.sound_level > previous.sound_level
                                || (event.sound_level == previous.sound_level && i > previous_index)
                        }) {
                            *sound = Some((i, event));
                        }
                    }
                }
            }
            for (id, sample) in targets {
                let nearby = sample.nearby;
                let sound = sample.sound.map(|(_, event)| event);
                let source = sound.map(|event| ContactActor {
                    actor: id,
                    cell: event.cell,
                    position: event.position,
                    rotation: [0.0; 3],
                    scale: 1.0,
                });
                let target = nearby
                    .or_else(|| {
                        actor_ids
                            .get(&id)
                            .copied()
                            .filter(|actor| self.space(actor.cell) == self.space(observer.cell))
                    })
                    .or(source.as_ref());
                let Some(target) = target else { continue };
                let light_level = if nearby.is_some() {
                    *levels
                        .entry(id)
                        .or_insert_with(|| light(target.cell, target.position))
                } else {
                    0.0
                };
                out.push(Observation {
                    observer: observer.actor,
                    target: id,
                    distance: length(sub(observer.position, target.position)),
                    line_of_sight: nearby.is_some() && visible(observer, target),
                    light_level,
                    motion: motions.get(&id).copied().unwrap_or_default(),
                    action_sound: sound.map_or(0.0, |event| event.sound_level),
                    action_sound_distance: sound
                        .map(|event| length(sub(observer.position, event.position))),
                    action_sound_line_of_sight: source
                        .as_ref()
                        .map(|source| visible(observer, source)),
                });
            }
        }
        out
    }
}

/// Keep placements that have another actor or a sound in range in the same cell.
pub fn candidate_actors(
    actors: Vec<ContactActor>,
    events: &[DetectionEvent],
    range: f32,
) -> Vec<ContactActor> {
    DetectionSpaces::separate_cells(range).candidate_actors(actors, events)
}

/// Ordered nearby actor pairs within one cell, sharing each target's light sample.
pub fn observations_in_range(
    actors: &[ContactActor],
    events: &[DetectionEvent],
    motions: &BTreeMap<GlobalFormId, Motion>,
    range: f32,
    visible: impl FnMut(&ContactActor, &ContactActor) -> bool,
    light: impl FnMut(GlobalFormId, [f32; 3]) -> f32,
) -> Vec<Observation> {
    DetectionSpaces::separate_cells(range).observations(actors, events, motions, visible, light)
}
