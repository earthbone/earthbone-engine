// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply saved INFO, SMQN and SCEN runtime markers to live records.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{GlobalFormId, Tag};
use soe_save::Save;

use super::record_overrides::{DialogueRecordExtra, RecordOverride};
use super::{SceneRunState, World};
use crate::vanilla_save::{RestoreError, SavedFormMap};

/// Number of decoded dialogue and scene changes applied from an ESS save.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DialogueSceneRestoreReport {
    pub infos: usize,
    pub story_manager_quest_nodes: usize,
    pub scenes: usize,
}

fn invalid(id: GlobalFormId, reason: impl Into<String>) -> RestoreError {
    RestoreError::DialogueScene {
        id,
        reason: reason.into(),
    }
}

impl World {
    /// Apply decoded INFO, SMQN and SCEN state after validating every target.
    ///
    /// INFO markers update dialogue history, SCEN markers update the playing
    /// scene set, and SMQN values remain available on their record override.
    /// INFO has no saved timestamp, so a true marker uses the current game day.
    /// The operation leaves the world unchanged if any mapped record is
    /// missing, deleted, duplicated or has the wrong type.
    ///
    /// # Errors
    /// Invalid plugin mapping, malformed payloads, duplicate targets or
    /// missing and mismatched records.
    pub fn restore_vanilla_dialogue_scene_changes(
        &mut self,
        save: &Save,
    ) -> Result<DialogueSceneRestoreReport, RestoreError> {
        let mapping = SavedFormMap::new(save, self.order())?;
        let mut overrides = BTreeMap::new();
        let mut said = BTreeMap::new();
        let mut scene_phases = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut report = DialogueSceneRestoreReport::default();

        for change in &save.change_forms {
            let (tag, extra, scene_phase) = match change.record_type() {
                "INFO" => {
                    let Some(data) = change.info_change()? else {
                        continue;
                    };
                    (
                        Tag(*b"INFO"),
                        DialogueRecordExtra {
                            form_flags: data.form_flags.map(|flags| (flags.flags, flags.unknown)),
                            info_said_once: Some(data.said_once),
                            ..DialogueRecordExtra::default()
                        },
                        None,
                    )
                }
                "SMQN" => {
                    let Some(data) = change.story_manager_quest_node_change()? else {
                        continue;
                    };
                    (
                        Tag(*b"SMQN"),
                        DialogueRecordExtra {
                            form_flags: data.form_flags.map(|flags| (flags.flags, flags.unknown)),
                            smqn_last_run_times: data.last_run_times,
                            ..DialogueRecordExtra::default()
                        },
                        None,
                    )
                }
                "SCEN" => {
                    let Some(data) = change.scene_change()? else {
                        continue;
                    };
                    let scene_phase = data.runtime_data()?.map(|runtime| runtime.phase);
                    (
                        Tag(*b"SCEN"),
                        DialogueRecordExtra {
                            form_flags: data.form_flags.map(|flags| (flags.flags, flags.unknown)),
                            scene_active: Some(data.active),
                            scene_active_data: data.active.then_some(data.active_data),
                            ..DialogueRecordExtra::default()
                        },
                        scene_phase,
                    )
                }
                _ => continue,
            };
            let id = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            if !seen.insert(id) {
                return Err(invalid(id, "duplicate dialogue or scene change form"));
            }
            let record = self
                .order()
                .winner(id)
                .filter(|record| !record.is_deleted() && record.tag == tag)
                .ok_or_else(|| invalid(id, format!("missing live {tag} record")))?;
            if tag == Tag(*b"SCEN") {
                let scene = soe_esm::records::gameplay::Scene::from_record(record)
                    .map_err(|error| invalid(id, error.to_string()))?;
                if extra.scene_active == Some(true) {
                    let Some(phase) = scene_phase else {
                        return Err(invalid(id, "active SCEN has no saved phase"));
                    };
                    if scene.phases.is_empty() || phase as usize >= scene.phases.len() {
                        return Err(invalid(
                            id,
                            format!(
                                "active SCEN phase {phase} exceeds {} record phases",
                                scene.phases.len()
                            ),
                        ));
                    }
                    if scene.actions.iter().any(|action| {
                        match (action.start_phase, action.end_phase) {
                            (Some(start), Some(end)) => {
                                start > end || end as usize >= scene.phases.len()
                            }
                            _ => true,
                        }
                    }) {
                        return Err(invalid(id, "active SCEN has invalid phase actions"));
                    }
                }
            }
            if self.record_overrides.contains_key(&id) {
                return Err(invalid(id, "record already has a runtime override"));
            }

            match tag {
                value if value == Tag(*b"INFO") => {
                    said.insert(id, extra.info_said_once.unwrap_or(false));
                    report.infos += 1;
                }
                value if value == Tag(*b"SMQN") => {
                    report.story_manager_quest_nodes += 1;
                }
                _ => {
                    report.scenes += 1;
                    if let Some(phase) = scene_phase {
                        scene_phases.insert(id, phase);
                    }
                }
            }
            let mut value = RecordOverride::new(id, record.clone());
            if let Some((flags, _)) = extra.form_flags {
                value.record.flags ^= flags;
            }
            value.dialogue = Some(extra);
            overrides.insert(id, value);
        }

        let mut dialogue_history = self.dialogue.said.clone();
        for (id, active) in said {
            if active {
                dialogue_history.insert(id, self.clock.game_days);
            } else {
                dialogue_history.remove(&id);
            }
        }
        let mut playing_scenes = self.scenes.clone();
        let mut playing_runs = self.scene_runs.clone();
        for id in overrides.keys() {
            if let Some(extra) = overrides.get(id).and_then(|value| value.dialogue.as_ref()) {
                if let Some(active) = extra.scene_active {
                    if active {
                        playing_scenes.insert(*id);
                        playing_runs.insert(
                            *id,
                            SceneRunState {
                                phase: scene_phases.get(id).copied().unwrap_or_default(),
                                ..SceneRunState::default()
                            },
                        );
                    } else {
                        playing_scenes.remove(id);
                        playing_runs.remove(id);
                    }
                }
            }
        }

        self.record_overrides.extend(overrides);
        self.dialogue.said = dialogue_history;
        self.scenes = playing_scenes;
        self.scene_runs = playing_runs;
        Ok(report)
    }
}
