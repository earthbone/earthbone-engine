// SPDX-License-Identifier: GPL-3.0-or-later

//! Streamed characters follow World contacts, not authored DATA positions.
//! xEdit ACHR/NPC_/RACE/HDPT and OpenMW ESM4NpcAnimation supply the selection
//! rules. Godot objects stay on the main thread, asset decoding uses rayon.

use std::collections::BTreeMap;
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::Instant;

use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::appearance::ActorAppearance;

use crate::actor::{ActorAssets, ActorSurface};
use crate::data_source::DataSource;
use crate::godot_actor::SkinnedActorNode;

use super::PlayerNode;

type RigCache = Arc<Mutex<BTreeMap<(String, String), Arc<ActorAssets>>>>;
type Prepared = Result<(Arc<ActorAssets>, Vec<ActorSurface>), String>;

#[derive(Default)]
struct LoadingTasks {
    active: Mutex<usize>,
    idle: Condvar,
}

struct LoadingTask(Arc<LoadingTasks>);

impl LoadingTasks {
    fn begin(self: &Arc<Self>) -> Result<LoadingTask, String> {
        let mut active = self.active.lock().map_err(|error| error.to_string())?;
        *active = active.checked_add(1).ok_or("too many NPC loading tasks")?;
        Ok(LoadingTask(Arc::clone(self)))
    }

    fn wait(&self) {
        if let Ok(mut active) = self.active.lock() {
            while *active != 0 {
                match self.idle.wait(active) {
                    Ok(next) => active = next,
                    Err(_) => break,
                }
            }
        }
    }
}

impl Drop for LoadingTask {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.active.lock() {
            *active = active.saturating_sub(1);
            self.0.idle.notify_all();
        }
    }
}

struct Resident {
    appearance: ActorAppearance,
    pending: Option<mpsc::Receiver<Prepared>>,
    node: Option<Gd<SkinnedActorNode>>,
    error: Option<String>,
}

/// Render resources and decoded rig cache. All gameplay state is supplied by World.
#[derive(Default)]
pub(super) struct NpcActors {
    residents: BTreeMap<GlobalFormId, Resident>,
    source: Option<Arc<DataSource>>,
    loading: Arc<LoadingTasks>,
    rigs: RigCache,
    appearances: BTreeMap<GlobalFormId, ActorAppearance>,
    next_appearance: Option<Instant>,
    hidden: bool,
    expected: usize,
    last_sync_us: f64,
}

impl NpcActors {
    pub(super) fn wait_for_loads(&self) {
        self.loading.wait();
    }

    pub(super) fn clear(&mut self) {
        for (_, resident) in std::mem::take(&mut self.residents) {
            if let Some(mut node) = resident.node {
                node.queue_free();
            }
        }
        self.appearances.clear();
        self.next_appearance = None;
    }
}

fn prepare(source: &DataSource, appearance: &ActorAppearance, rigs: &RigCache) -> Prepared {
    let race = appearance.race.ok_or("NPC has no race")?;
    let record = source.order().winner(race).ok_or("NPC race is missing")?;
    let models =
        soe_game::race::AnimationModels::from_record(record).map_err(|error| error.to_string())?;
    let selected = models.select(appearance.female);
    let key = (
        selected.skeleton.clone().unwrap_or_default(),
        selected.behavior.clone().unwrap_or_default(),
    );
    let rig = {
        let mut cache = rigs.lock().map_err(|error| error.to_string())?;
        if let Some(rig) = cache.get(&key) {
            Arc::clone(rig)
        } else {
            let record = source.order().winner(race).ok_or("NPC race is missing")?;
            let empty = ActorAppearance {
                race: Some(race),
                female: appearance.female,
                ..Default::default()
            };
            let mut rig =
                ActorAssets::load_for_race(source.vfs(), record, appearance.female, &empty)
                    .map_err(|error| error.to_string())?;
            rig.complete_race_skeleton(source.vfs())
                .map_err(|error| error.to_string())?;
            rig.actor_race = Some(race);
            let rig = Arc::new(rig);
            cache.insert(key, Arc::clone(&rig));
            rig
        }
    };
    let surfaces = ActorAssets::appearance_surfaces(source.vfs(), appearance, &rig.skeleton)
        .map_err(|error| error.to_string())?;
    if surfaces.is_empty() {
        return Err("NPC appearance contains no skinned surfaces".into());
    }
    let unmatched: std::collections::BTreeSet<_> = surfaces
        .iter()
        .flat_map(|surface| surface.map.unmatched.iter().cloned())
        .collect();
    if !unmatched.is_empty() {
        return Err(format!("NPC rig has unmatched bones: {unmatched:?}"));
    }
    Ok((rig, surfaces))
}

impl PlayerNode {
    pub(super) fn sync_npc_actors(&mut self) {
        let _scope = soe_trace::SystemScope::new(soe_trace::System::Render);
        let started = Instant::now();
        let Some(source) = self.data_source.as_ref().map(Arc::clone) else {
            return;
        };
        if self
            .npc_actors
            .source
            .as_ref()
            .is_none_or(|prior| !Arc::ptr_eq(prior, &source))
        {
            self.npc_actors.clear();
            self.npc_actors.rigs = RigCache::default();
            self.npc_actors.source = Some(Arc::clone(&source));
        }
        let mut placements = BTreeMap::new();
        if let Some(pair) = &self.simulation_pair {
            self.npc_actors
                .appearances
                .clone_from(&pair.current.appearances);
            for contact in pair.current.contacts.iter() {
                if contact.actor == pair.current.player {
                    continue;
                }
                if let Some(mut placement) = pair.placement(contact.actor, started) {
                    placement.scale = contact.scale;
                    placements.insert(contact.actor, placement);
                }
            }
        } else if let Some(game) = &self.game {
            let mut world = game.world_mut();
            let contacts = world.contact_actors();
            let player = world.player();
            let refresh = self
                .npc_actors
                .next_appearance
                .is_none_or(|next| started >= next);
            if refresh {
                self.npc_actors.appearances.clear();
                self.npc_actors.next_appearance =
                    Some(started + std::time::Duration::from_millis(250));
            }
            for contact in contacts {
                if contact.actor == player {
                    continue;
                }
                if refresh || !self.npc_actors.appearances.contains_key(&contact.actor) {
                    match world.actor_appearance(contact.actor) {
                        Ok(appearance) => {
                            self.npc_actors
                                .appearances
                                .insert(contact.actor, appearance);
                        }
                        Err(error) => {
                            soe_trace::error!(target: "host::actor", system = "host::actor", "{} appearance: {error}", contact.actor);
                        }
                    }
                }
                if let Some(mut placement) = world.ref_placement(contact.actor) {
                    placement.scale = contact.scale;
                    placement.visible &= !world.is_invisible(contact.actor);
                    placements.insert(contact.actor, placement);
                }
            }
        }
        placements.retain(|id, placement| {
            let cell = if let Some(pair) = &self.simulation_pair {
                pair.current
                    .contacts
                    .iter()
                    .find(|contact| contact.actor == *id)
                    .map(|contact| contact.cell)
            } else {
                self.game.as_ref().and_then(|game| {
                    game.world()
                        .reference_ref(*id)
                        .and_then(|reference| reference.cell)
                })
            };
            cell.is_some_and(|cell| {
                self.game_streamer.as_ref().is_some_and(|streamer| {
                    streamer
                        .bind()
                        .actor_cell_is_resident(cell, placement.position)
                })
            })
        });
        self.npc_actors.expected = placements
            .keys()
            .filter(|id| {
                source
                    .order()
                    .winner(**id)
                    .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
            })
            .count();
        let departed: Vec<_> = self
            .npc_actors
            .residents
            .keys()
            .filter(|id| !placements.contains_key(id))
            .copied()
            .collect();
        for id in departed {
            if let Some(resident) = self.npc_actors.residents.remove(&id) {
                if let Some(mut node) = resident.node {
                    node.queue_free();
                }
            }
        }
        let mut parent = self.base().clone();
        let mut uploaded = false;
        for (id, placement) in placements {
            let Some(appearance) = self.npc_actors.appearances.get(&id) else {
                continue;
            };
            if source
                .order()
                .winner(id)
                .is_none_or(|record| record.tag != soe_esm::tag::ACHR)
            {
                continue;
            }
            let resident = self
                .npc_actors
                .residents
                .entry(id)
                .or_insert_with(|| Resident {
                    appearance: appearance.clone(),
                    pending: None,
                    node: None,
                    error: None,
                });
            if resident.appearance != *appearance {
                resident.appearance.clone_from(appearance);
                resident.pending = None;
                resident.error = None;
            }
            if resident.pending.is_none()
                && resident.error.is_none()
                && (resident.node.is_none()
                    || resident
                        .node
                        .as_ref()
                        .is_some_and(|node| node.bind().appearance_outdated(appearance)))
            {
                let task = match self.npc_actors.loading.begin() {
                    Ok(task) => task,
                    Err(error) => {
                        resident.error = Some(error);
                        continue;
                    }
                };
                let (sender, receiver) = mpsc::sync_channel(1);
                let source = Arc::clone(&source);
                let view = appearance.clone();
                let rigs = Arc::clone(&self.npc_actors.rigs);
                rayon::spawn(move || {
                    let _task = task;
                    let _ = sender.send(prepare(&source, &view, &rigs));
                });
                resident.pending = Some(receiver);
            }
            if !uploaded {
                if let Some(result) = resident
                    .pending
                    .as_ref()
                    .and_then(|pending| pending.try_recv().ok())
                {
                    resident.pending = None;
                    match result {
                        Ok((rig, surfaces)) => {
                            let mut node = resident
                                .node
                                .take()
                                .unwrap_or_else(SkinnedActorNode::new_alloc);
                            if !node.is_inside_tree() {
                                parent.add_child(&node);
                            }
                            node.bind_mut().present(placement, self.npc_actors.hidden);
                            let result = node.bind_mut().install_presented_actor(
                                id,
                                rig,
                                surfaces,
                                appearance.clone(),
                            );
                            if let Some(game) = &self.game {
                                node.bind_mut().bind_presented_world(
                                    game.shared_world(),
                                    Arc::clone(&source),
                                    id,
                                );
                            }
                            if let Err(error) = result {
                                resident.error = Some(error);
                                node.queue_free();
                            } else {
                                resident.node = Some(node);
                            }
                            uploaded = true;
                        }
                        Err(error) => {
                            godot_warn!("NPC {id}: {error}");
                            resident.error = Some(error);
                        }
                    }
                }
            }
            if let Some(node) = resident.node.as_mut() {
                if let Some(game) = &self.game {
                    node.bind_mut().bind_presented_world(
                        game.shared_world(),
                        Arc::clone(&source),
                        id,
                    );
                }
                node.bind_mut().present(placement, self.npc_actors.hidden);
            }
        }
        soe_trace::metric_set(
            soe_trace::Metric::AnimationActors,
            self.npc_actors
                .residents
                .values()
                .filter(|resident| resident.node.is_some())
                .count() as u64,
        );
        self.npc_actors.last_sync_us = started.elapsed().as_secs_f64() * 1e6;
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Resident NPC identities, skinned surface counts, transforms and loader failures.
    #[func]
    pub fn npc_visuals(&self) -> Array<VarDictionary> {
        self.npc_actors.residents.iter().map(|(id, resident)| {
            let (shapes, bones, position, node_id) = resident.node.as_ref().map_or((0, 0, Vector3::ZERO, 0), |node| {
                let node = node.bind();
                let stats = node.stats();
                (stats.get("shapes").and_then(|value| value.try_to::<i64>().ok()).unwrap_or(0),
                 stats.get("mesh_bones").and_then(|value| value.try_to::<i64>().ok()).unwrap_or(0),
                 node.presented_position(), node.node_identity())
            });
            let editor_id = self.data_source.as_ref().and_then(|source| source.order().winner(*id))
                .and_then(soe_esm::Record::editor_id).unwrap_or_default();
            vdict! { "editor_id" => editor_id.as_str(), "file" => i64::from(id.file), "object" => i64::from(id.object),
                "shapes" => shapes, "bones" => bones, "position" => position,
                "node_id" => node_id, "pending" => resident.pending.is_some(),
                "head_parts" => resident.appearance.head_parts.len() as i64,
                "armor_parts" => resident.appearance.parts.len() as i64,
                "pose_us" => resident.node.as_ref().map_or(0.0, |node| node.bind().pose_us()),
                "error" => resident.error.as_deref().unwrap_or("") }
        }).collect()
    }

    /// Hide all NPC RIDs for a paired capture without changing gameplay state.
    #[func]
    pub fn set_npc_visuals_hidden(&mut self, hidden: bool) {
        self.npc_actors.hidden = hidden;
        self.sync_npc_actors();
    }

    /// Number of enabled resident ACHRs expected and currently drawn.
    #[func]
    pub fn npc_visual_counts(&self) -> VarDictionary {
        vdict! { "expected" => self.npc_actors.expected as i64,
        "drawn" => self.npc_actors.residents.values().filter(|resident| resident.node.is_some()).count() as i64,
        "pending" => self.npc_actors.residents.values().filter(|resident| resident.pending.is_some()).count() as i64,
        "failed" => self.npc_actors.residents.values().filter(|resident| resident.error.is_some()).count() as i64 }
    }

    /// Completed NPC reconciliation and total pose cost, in microseconds.
    #[func]
    pub fn npc_visual_timing(&self) -> VarDictionary {
        let pose: f64 = self
            .npc_actors
            .residents
            .values()
            .filter_map(|resident| resident.node.as_ref().map(|node| node.bind().pose_us()))
            .sum();
        vdict! { "sync_us" => self.npc_actors.last_sync_us, "pose_us" => pose }
    }

    /// Last completed resident reconciliation time, in microseconds.
    #[func]
    pub fn npc_visual_sync_us(&self) -> f64 {
        self.npc_actors.last_sync_us
    }
}

#[cfg(test)]
mod hst_23_tests {
    use super::LoadingTasks;
    use std::sync::{mpsc, Arc};

    #[test]
    fn hst_23_shutdown_waits_for_all_asset_jobs() {
        let tasks = Arc::new(LoadingTasks::default());
        let guard = tasks.begin().unwrap();
        let (finished, receiver) = mpsc::channel();
        let retained = Arc::clone(&tasks);
        let waiter = std::thread::spawn(move || {
            retained.wait();
            finished.send(()).unwrap();
        });
        assert!(receiver.try_recv().is_err());
        drop(guard);
        waiter.join().unwrap();
        receiver.recv().unwrap();
    }

    #[test]
    fn hst_23_cancelled_decoder_releases_its_shutdown_guard() {
        let tasks = Arc::new(LoadingTasks::default());
        let guard = tasks.begin().unwrap();
        drop(guard);
        tasks.wait();
        assert_eq!(*tasks.active.lock().unwrap(), 0);
    }
}
