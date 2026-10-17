// SPDX-License-Identifier: GPL-3.0-or-later

//! Full-model replacement keeps old RIDs until its upload slices finish.
//! Mudcrab's acknowledged drawable lifecycle supplies the handoff rule.

use std::collections::BTreeMap;
use std::thread::JoinHandle;

use super::{upload, BatchTable, CellStreamerNode, LoadedCell, LoadedSurface, StreamTotals};
use godot::prelude::*;
use soe_esm::GlobalFormId;

pub(super) struct Prepared {
    loaded: LoadedCell,
    plan: upload::Plan,
    surfaces: BTreeMap<GlobalFormId, Vec<LoadedSurface>>,
}

pub(super) struct Publishing {
    prepared: Prepared,
    totals: StreamTotals,
    old_rids: Vec<Rid>,
    new_rids: Vec<(Rid, usize)>,
}

impl Publishing {
    pub(super) fn contains_rid(&self, rid: Rid) -> bool {
        self.new_rids.iter().any(|(pending, _)| *pending == rid)
    }
}

fn owns_batch(table: &BatchTable, surfaces: &BTreeMap<GlobalFormId, Vec<LoadedSurface>>) -> bool {
    match table {
        BatchTable::Instances(members) => {
            !members.is_empty() && members.iter().all(|id| surfaces.contains_key(id))
        }
        BatchTable::Triangles(members) => {
            !members.is_empty()
                && members
                    .iter()
                    .all(|member| surfaces.contains_key(&member.reference))
        }
    }
}

pub(super) type Worker = JoinHandle<Result<Prepared, String>>;

impl CellStreamerNode {
    pub(super) fn request_distant_upload(
        &mut self,
        owner: GlobalFormId,
        additions: Vec<GlobalFormId>,
        selected: Vec<GlobalFormId>,
    ) -> bool {
        let Some(source) = self.data_source.clone() else {
            return false;
        };
        let mut surfaces = self.distant.surfaces.clone();
        surfaces.retain(|id, _| selected.contains(id));
        let batch = self.batch_surfaces;
        match std::thread::Builder::new()
            .name("distant-prepare".into())
            .spawn(move || {
                let _span = soe_trace::Span::new("host.distant_prepare", "host");
                let mut loaded = source
                    .stream_selected_references(owner, &additions)
                    .map_err(|error| error.to_string())?;
                for id in &additions {
                    surfaces.entry(*id).or_default();
                }
                for surface in loaded.surfaces.drain(..) {
                    surfaces.entry(surface.reference).or_default().push(surface);
                }
                loaded.surfaces = surfaces.values().flatten().cloned().collect();
                loaded.stats.references = selected.len();
                loaded.stats.triangles = loaded
                    .surfaces
                    .iter()
                    .map(|s| s.arrays.triangle_count())
                    .sum();
                let plan = upload::Plan::new(Some(&source), owner, &loaded.surfaces, batch);
                Ok(Prepared {
                    loaded,
                    plan,
                    surfaces,
                })
            }) {
            Ok(worker) => {
                self.distant.worker = Some(worker);
                true
            }
            Err(error) => {
                soe_trace::error!(target: "host::distant", system = "cell_streaming", "distant worker: {error}");
                false
            }
        }
    }

    /// Returns whether a replacement is still pending after this frame's slice.
    pub(super) fn poll_distant_upload(&mut self, owner: GlobalFormId) -> Result<bool, ()> {
        if self
            .distant
            .worker
            .as_ref()
            .is_some_and(JoinHandle::is_finished)
        {
            let Some(worker) = self.distant.worker.take() else {
                return Err(());
            };
            let prepared = match worker.join() {
                Ok(Ok(prepared)) => prepared,
                Ok(Err(error)) => {
                    soe_trace::error!(target: "host::distant", system = "cell_streaming", "distant preparation: {error}");
                    return Err(());
                }
                Err(_) => return Err(()),
            };
            let old_rids = self
                .instances
                .iter()
                .filter(|instance| {
                    instance.cell == owner && owns_batch(&instance.table, &self.distant.surfaces)
                })
                .map(|instance| instance.rid)
                .collect();
            self.distant.publishing = Some(Publishing {
                prepared,
                totals: StreamTotals::default(),
                old_rids,
                new_rids: Vec::new(),
            });
        }
        if let Some(mut publishing) = self.distant.publishing.take() {
            let _capture = publishing.prepared.loaded.stats.profile.enter();
            let first = self.instances.len();
            if !self.upload_plan_step(
                owner,
                &mut publishing.prepared.plan,
                &mut publishing.totals,
                upload::SLICE_BUDGET,
            ) {
                return Err(());
            }
            let mut server = super::RenderingServer::singleton();
            for instance in &self.instances[first..] {
                publishing
                    .new_rids
                    .push((instance.rid, instance.render_kind));
                server.instance_set_visible(instance.rid, false);
            }
            if !publishing.prepared.plan.is_empty() {
                self.distant.publishing = Some(publishing);

                return Ok(true);
            }
            let mut server = super::RenderingServer::singleton();
            self.instances.retain(|instance| {
                if publishing.old_rids.contains(&instance.rid) {
                    server.free_rid(instance.rid);
                    false
                } else {
                    true
                }
            });
            for (rid, kind) in publishing.new_rids {
                server.instance_set_visible(rid, self.console_visibility[kind]);
            }
            self.occluders.retain(|occluder| {
                occluder.cell != owner
                    || !occluder
                        .sources
                        .keys()
                        .all(|id| self.distant.surfaces.contains_key(id))
            });
            if self.cell_occluders {
                if let Some(scenario) = self.scenario() {
                    self.add_cell_occluder(owner, &publishing.prepared.loaded.surfaces, scenario);
                }
            }
            let stale_doors: Vec<_> = self
                .doors
                .keys()
                .filter(|id| {
                    self.distant.surfaces.contains_key(*id)
                        && !publishing.prepared.surfaces.contains_key(*id)
                })
                .copied()
                .collect();
            for id in stale_doors {
                if let Some(door) = self.doors.remove(&id) {
                    door.free();
                }
                self.ref_cache.remove(&id);
            }
            self.distant.surfaces = publishing.prepared.surfaces;
            self.reference_index_dirty = true;
            self.materials.prune_unused();
        }

        Ok(self.distant.worker.is_some() || self.distant.publishing.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_40_distant_replacement_preserves_other_persistent_geometry() {
        let distant = GlobalFormId { file: 0, object: 1 };
        let ordinary = GlobalFormId { file: 0, object: 2 };
        let surfaces = BTreeMap::from([(distant, Vec::new())]);
        assert!(owns_batch(&BatchTable::Instances(vec![distant]), &surfaces));
        assert!(!owns_batch(
            &BatchTable::Instances(vec![ordinary]),
            &surfaces
        ));
        assert!(!owns_batch(
            &BatchTable::Instances(vec![distant, ordinary]),
            &surfaces
        ));
        assert!(!owns_batch(&BatchTable::Instances(Vec::new()), &surfaces));
        assert!(!owns_batch(
            &BatchTable::Instances(vec![distant]),
            &BTreeMap::new()
        ));
    }
}
