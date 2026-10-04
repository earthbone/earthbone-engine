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

/// Ordered nearby actor pairs and sounds within detection range. Sight and
/// light are sampled only for nearby actors, once per target for light.
/// A nearby sound can still belong to a distant or nonresident actor.
pub fn observations_in_range(
    actors: &[ContactActor],
    events: &[DetectionEvent],
    motions: &BTreeMap<GlobalFormId, Motion>,
    range: f32,
    mut visible: impl FnMut(&ContactActor, &ContactActor) -> bool,
    mut light: impl FnMut(GlobalFormId, [f32; 3]) -> f32,
) -> Vec<Observation> {
    if !range.is_finite() || range <= 0.0 {
        return Vec::new();
    }
    let mut actor_grid: HashMap<Bucket, Vec<usize>> = HashMap::new();
    let mut event_grid: HashMap<Bucket, Vec<usize>> = HashMap::new();
    let mut actor_ids = BTreeMap::new();
    for (i, actor) in actors.iter().enumerate() {
        actor_grid
            .entry((actor.cell, bucket(actor.position, range)))
            .or_default()
            .push(i);
        actor_ids.insert(actor.actor, actor);
    }
    for (i, event) in events.iter().enumerate() {
        event_grid
            .entry((event.cell, bucket(event.position, range)))
            .or_default()
            .push(i);
    }
    let mut levels = BTreeMap::new();
    let mut out = Vec::new();
    for observer in actors {
        let mut targets: BTreeMap<GlobalFormId, TargetSample<'_>> = BTreeMap::new();
        for key in neighbors(bucket(observer.position, range)) {
            if let Some(indices) = actor_grid.get(&(observer.cell, key)) {
                for &i in indices {
                    let target = &actors[i];
                    if target.actor != observer.actor
                        && length(sub(observer.position, target.position)) <= range
                    {
                        targets.entry(target.actor).or_default().nearby = Some(target);
                    }
                }
            }
            if let Some(indices) = event_grid.get(&(observer.cell, key)) {
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
                        .filter(|actor| actor.cell == observer.cell)
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
                action_sound_line_of_sight: source.as_ref().map(|source| visible(observer, source)),
            });
        }
    }
    out
}
