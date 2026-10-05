// SPDX-License-Identifier: GPL-3.0-or-later

//! ActorUtil package overrides from PapyrusUtil's ActorUtil.psc and
//! PackageData.cpp. Ordered entries preserve the script contract's latest tie.

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use super::save::{read_form_id, write_form_id, FormReader};
use super::{ActorState, World};

/// One ActorUtil override, ordered by addition on the owning actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageOverride {
    pub package: GlobalFormId,
    pub priority: u8,
    /// ActorUtil flags value 1 bypasses package eligibility checks.
    pub force: bool,
}

impl World {
    /// Add or replace an ActorUtil override. Priority is clamped to 0 to 100.
    /// Equal priorities choose the last added entry. Evaluation occurs next tick.
    /// Returns false for an unavailable actor or invalid PACK.
    pub fn add_package_override(
        &mut self,
        actor: GlobalFormId,
        package: GlobalFormId,
        priority: i32,
        flags: i32,
    ) -> bool {
        if self
            .record(package)
            .is_none_or(|record| record.is_deleted() || record.tag != Tag(*b"PACK"))
        {
            return false;
        }
        let Some(state) = self.actor(actor) else {
            return false;
        };
        state
            .runtime_package_overrides
            .retain(|entry| entry.package != package);
        state.runtime_package_overrides.push(PackageOverride {
            package,
            priority: priority.clamp(0, 100) as u8,
            force: flags == 1,
        });
        state.ai.reevaluate = true;
        true
    }

    /// Remove an ActorUtil override without changing SPID or authored packages.
    /// Returns whether an entry was removed. Evaluation occurs next tick.
    pub fn remove_package_override(&mut self, actor: GlobalFormId, package: GlobalFormId) -> bool {
        let Some(state) = self.actor(actor) else {
            return false;
        };
        let before = state.runtime_package_overrides.len();
        state
            .runtime_package_overrides
            .retain(|entry| entry.package != package);
        let removed = state.runtime_package_overrides.len() != before;
        state.ai.reevaluate |= removed;
        removed
    }

    /// Count overrides including entries whose eligibility checks fail.
    pub fn count_package_overrides(&self, actor: GlobalFormId) -> usize {
        self.actor_ref(actor)
            .map_or(0, |state| state.runtime_package_overrides.len())
    }

    /// Clear all ActorUtil overrides on an actor and return the removed count.
    pub fn clear_package_overrides(&mut self, actor: GlobalFormId) -> usize {
        let Some(state) = self.actor(actor) else {
            return 0;
        };
        let count = state.runtime_package_overrides.len();
        state.runtime_package_overrides.clear();
        state.ai.reevaluate |= count != 0;
        count
    }

    /// Remove a package from every built actor's overrides, returning actor count.
    pub fn remove_all_package_overrides(&mut self, package: GlobalFormId) -> usize {
        self.loaded_actors()
            .into_iter()
            .filter(|actor| self.remove_package_override(*actor, package))
            .count()
    }

    /// CK SetPlayerAIDriven, sharing the flag used by the package driver.
    pub fn set_player_ai_driven(&mut self, driven: bool) {
        let player = self.player();
        if driven {
            self.drive_actor(player);
        }
        self.player_ai_driven = driven;
    }
}

pub(crate) fn write_overrides(state: &ActorState, writer: &mut Writer, _pool: &mut ArrayPool) {
    if state.runtime_package_overrides.is_empty() {
        return;
    }
    writer.len(state.runtime_package_overrides.len());
    for entry in &state.runtime_package_overrides {
        write_form_id(writer, entry.package);
        writer.u8(entry.priority);
        writer.bool(entry.force);
    }
}

pub(crate) fn read_overrides(
    state: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    for _ in 0..count {
        let package = read_form_id(reader)?;
        let priority = reader.u8()?;
        let force = reader.bool()?;
        if priority > 100
            || state
                .runtime_package_overrides
                .iter()
                .any(|entry| entry.package == package)
        {
            return Err(VmError::State(
                "invalid package override priority or duplicate".into(),
            ));
        }
        state.runtime_package_overrides.push(PackageOverride {
            package,
            priority,
            force,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::save::FormNamespace;
    use super::super::save_sections::{read_sections, write_sections, Section};
    use super::*;
    use soe_papyrus::vm::state::Reader;

    const SECTION: Section<ActorState> = Section {
        name: "PLY-06",
        write: write_overrides,
        read: read_overrides,
    };

    fn read(bytes: &[u8], namespace: FormNamespace) -> Result<ActorState, VmError> {
        let mut state = ActorState::default();
        let mut reader = Reader::new(bytes);
        read_sections(
            &mut FormReader::new(&mut reader, namespace),
            "actor",
            &mut state,
            &[SECTION],
        )?;
        Ok(state)
    }

    #[test]
    fn ply_06_actor_section_absence_leaves_no_overrides() {
        let mut writer = Writer::new();
        write_sections(&mut writer, &ActorState::default(), &[SECTION]);
        assert_eq!(writer.out, [0, 0]);
        assert!(read(&writer.out, FormNamespace::Current)
            .unwrap()
            .runtime_package_overrides
            .is_empty());
    }

    #[test]
    fn ply_06_actor_section_reports_corruption_without_accepting_partial_payloads() {
        for (priority, entries, truncate, trailing) in [
            (101, 1, false, false),
            (30, 2, false, false),
            (30, 1, true, false),
            (30, 1, false, true),
        ] {
            let mut payload = Writer::new();
            payload.len(entries);
            for _ in 0..entries {
                write_form_id(
                    &mut payload,
                    GlobalFormId {
                        file: 0,
                        object: 0x810,
                    },
                );
                payload.u8(priority);
                payload.bool(false);
            }
            if truncate {
                payload.out.pop();
            }
            if trailing {
                payload.u8(1);
            }
            let mut writer = Writer::new();
            writer.u16(1);
            writer.u8(6);
            writer.out.extend_from_slice(b"PLY-06");
            writer.bytes(&payload.out);
            let error = read(&writer.out, FormNamespace::Current)
                .unwrap_err()
                .to_string();
            assert!(error.contains("PLY-06 at actor"), "{error}");
        }
    }

    #[test]
    fn ply_06_actor_section_reads_forms_through_its_enclosing_namespace() {
        let entry = PackageOverride {
            package: GlobalFormId {
                file: 0xff,
                object: 0x810,
            },
            priority: 30,
            force: true,
        };
        let mut state = ActorState::default();
        state.runtime_package_overrides.push(entry);
        let mut writer = Writer::new();
        write_sections(&mut writer, &state, &[SECTION]);
        assert_eq!(
            read(&writer.out, FormNamespace::Current)
                .unwrap()
                .runtime_package_overrides,
            [entry]
        );
        assert_eq!(
            read(&writer.out, FormNamespace::Legacy)
                .unwrap()
                .runtime_package_overrides[0]
                .package
                .file,
            super::super::DYNAMIC_FILE
        );
    }
}
