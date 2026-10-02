// SPDX-License-Identifier: GPL-3.0-or-later

//! Save and load for a whole [`Game`]: the world's run-time state and the
//! VM's, one after the other inside a small envelope of their own.
//!
//! Plugin and loaded-script fingerprints travel in the file as provenance.
//! The caller supplies those definitions before `load_state`. Plugin identity
//! changes reject loading, while VM script fingerprints determine whether
//! saved execution state loads or remains suspended.

use soe_esm::Slot;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;
use soe_save::{Papyrus, Save};

use crate::Game;

pub const MAGIC: &[u8; 4] = b"SOEG";
pub const VERSION: u16 = 2;

/// Dependency identity recorded in a native game save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveProvenance {
    pub plugins: Vec<PluginProvenance>,
    pub scripts: Vec<ScriptProvenance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginProvenance {
    pub name: String,
    pub light: bool,
    pub slot: u16,
    pub fingerprint: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptProvenance {
    pub name: String,
    pub fingerprint: Option<[u8; 32]>,
}

/// Dependency differences found before importing a saved world or VM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyReport {
    pub source: DependencySource,
    pub findings: Vec<DependencyFinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The save format whose dependency identities were checked.
pub enum DependencySource {
    Native,
    Vanilla,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The kind of content named by a dependency finding.
pub enum DependencyKind {
    Plugin,
    Script,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The saved state that depends on a plugin or script.
pub enum SavedStateKind {
    World,
    Vm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// The difference found between saved and current dependency information.
pub enum DependencyChange {
    Missing,
    Added,
    KindChanged,
    FingerprintChanged,
    SlotChanged,
    ProvenanceUnknown,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
/// One dependency identity and its effect on saved state.
pub struct DependencyFinding {
    pub kind: DependencyKind,
    pub identity: String,
    pub affects: SavedStateKind,
    pub change: DependencyChange,
}

impl SaveProvenance {
    fn write(&self, writer: &mut Writer) {
        writer.len(self.plugins.len());
        for plugin in &self.plugins {
            writer.str(&plugin.name);
            writer.bool(plugin.light);
            writer.u16(plugin.slot);
            writer.out.extend_from_slice(&plugin.fingerprint);
        }
        writer.len(self.scripts.len());
        for script in &self.scripts {
            writer.str(&script.name);
            writer.bool(script.fingerprint.is_some());
            if let Some(fingerprint) = script.fingerprint {
                writer.out.extend_from_slice(&fingerprint);
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, VmError> {
        let plugin_count = reader.len()?;
        let mut plugins = Vec::with_capacity(plugin_count);
        for _ in 0..plugin_count {
            let name = reader.str()?;
            let light = reader.bool()?;
            let slot = reader.u16()?;
            let mut fingerprint = [0; 32];
            fingerprint.copy_from_slice(reader.take(32)?);
            plugins.push(PluginProvenance {
                name,
                light,
                slot,
                fingerprint,
            });
        }
        let script_count = reader.len()?;
        let mut scripts = Vec::with_capacity(script_count);
        for _ in 0..script_count {
            let name = reader.str()?;
            let fingerprint = if reader.bool()? {
                let mut fingerprint = [0; 32];
                fingerprint.copy_from_slice(reader.take(32)?);
                Some(fingerprint)
            } else {
                None
            };
            scripts.push(ScriptProvenance { name, fingerprint });
        }
        Ok(Self { plugins, scripts })
    }
}

fn current_provenance(game: &Game) -> SaveProvenance {
    let mut plugins: Vec<_> = game
        .world
        .borrow()
        .order()
        .files()
        .iter()
        .map(|file| {
            let (light, slot) = match file.slot {
                Slot::Regular(slot) => (false, u16::from(slot)),
                Slot::Light(slot) => (true, slot),
            };
            PluginProvenance {
                name: file.name.clone(),
                light,
                slot,
                fingerprint: file.fingerprint,
            }
        })
        .collect();
    plugins.sort_by_key(|plugin| plugin.name.to_ascii_lowercase());
    let scripts = game
        .vm
        .loaded_script_fingerprints()
        .into_iter()
        .map(|(name, fingerprint)| ScriptProvenance { name, fingerprint })
        .collect();
    SaveProvenance { plugins, scripts }
}

fn read_envelope(bytes: &[u8]) -> Result<(SaveProvenance, &[u8], &[u8]), VmError> {
    let mut reader = Reader::new(bytes);
    if reader.take(4)? != MAGIC {
        return Err(VmError::State("not a game save file".into()));
    }
    let version = reader.u16()?;
    if version != VERSION {
        return Err(VmError::State(format!(
            "game save version {version}, this build reads {VERSION}"
        )));
    }
    let provenance = SaveProvenance::read(&mut reader)?;
    let world = reader.bytes()?;
    let vm = reader.bytes()?;
    reader.expect_end()?;
    Ok((provenance, world, vm))
}

fn check_provenance(saved: &SaveProvenance, current: &SaveProvenance) -> Result<(), VmError> {
    if saved.plugins != current.plugins {
        let saved_by_name: std::collections::BTreeMap<_, _> = saved
            .plugins
            .iter()
            .map(|item| (item.name.to_ascii_lowercase(), item))
            .collect();
        let current_by_name: std::collections::BTreeMap<_, _> = current
            .plugins
            .iter()
            .map(|item| (item.name.to_ascii_lowercase(), item))
            .collect();
        for (name, previous) in &saved_by_name {
            match current_by_name.get(name) {
                None => {
                    return Err(VmError::State(format!(
                        "saved plugin {} is missing",
                        previous.name
                    )))
                }
                Some(now) if previous.light != now.light => {
                    return Err(VmError::State(format!(
                        "saved plugin {} changed regular/light kind",
                        previous.name
                    )))
                }
                Some(now) if previous.fingerprint != now.fingerprint => {
                    return Err(VmError::State(format!(
                        "saved plugin {} content changed",
                        previous.name
                    )))
                }
                Some(now) if previous.slot != now.slot => {
                    return Err(VmError::State(format!(
                        "saved plugin {} moved load-order slot",
                        previous.name
                    )))
                }
                Some(_) => {}
            }
        }
        if let Some(extra) = current_by_name
            .keys()
            .find(|name| !saved_by_name.contains_key(*name))
        {
            return Err(VmError::State(format!(
                "current plugin {extra} was not in the save"
            )));
        }
    }
    Ok(())
}

fn native_dependency_report(saved: &SaveProvenance, current: &SaveProvenance) -> DependencyReport {
    let mut findings = Vec::new();
    let current_plugins: std::collections::BTreeMap<_, _> = current
        .plugins
        .iter()
        .map(|plugin| (plugin.name.to_ascii_lowercase(), plugin))
        .collect();
    let saved_plugins: std::collections::BTreeMap<_, _> = saved
        .plugins
        .iter()
        .map(|plugin| (plugin.name.to_ascii_lowercase(), plugin))
        .collect();
    for (key, previous) in &saved_plugins {
        let Some(now) = current_plugins.get(key) else {
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity: previous.name.clone(),
                affects: SavedStateKind::World,
                change: DependencyChange::Missing,
            });
            continue;
        };
        if previous.light != now.light {
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity: previous.name.clone(),
                affects: SavedStateKind::World,
                change: DependencyChange::KindChanged,
            });
        }
        if previous.fingerprint != now.fingerprint {
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity: previous.name.clone(),
                affects: SavedStateKind::World,
                change: DependencyChange::FingerprintChanged,
            });
        }
        if previous.slot != now.slot {
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity: previous.name.clone(),
                affects: SavedStateKind::World,
                change: DependencyChange::SlotChanged,
            });
        }
    }
    for (key, plugin) in &current_plugins {
        if !saved_plugins.contains_key(key) {
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity: plugin.name.clone(),
                affects: SavedStateKind::World,
                change: DependencyChange::Added,
            });
        }
    }

    let current_scripts: std::collections::BTreeMap<_, _> = current
        .scripts
        .iter()
        .map(|script| (script.name.to_ascii_lowercase(), script))
        .collect();
    let saved_scripts: std::collections::BTreeMap<_, _> = saved
        .scripts
        .iter()
        .map(|script| (script.name.to_ascii_lowercase(), script))
        .collect();
    for (key, previous) in &saved_scripts {
        let Some(now) = current_scripts.get(key) else {
            findings.push(DependencyFinding {
                kind: DependencyKind::Script,
                identity: previous.name.clone(),
                affects: SavedStateKind::Vm,
                change: DependencyChange::Missing,
            });
            if previous.fingerprint.is_none() {
                findings.push(DependencyFinding {
                    kind: DependencyKind::Script,
                    identity: previous.name.clone(),
                    affects: SavedStateKind::Vm,
                    change: DependencyChange::ProvenanceUnknown,
                });
            }
            continue;
        };
        match (previous.fingerprint, now.fingerprint) {
            (Some(old), Some(new)) if old != new => findings.push(DependencyFinding {
                kind: DependencyKind::Script,
                identity: previous.name.clone(),
                affects: SavedStateKind::Vm,
                change: DependencyChange::FingerprintChanged,
            }),
            (None, _) | (_, None) => findings.push(DependencyFinding {
                kind: DependencyKind::Script,
                identity: previous.name.clone(),
                affects: SavedStateKind::Vm,
                change: DependencyChange::ProvenanceUnknown,
            }),
            _ => {}
        }
    }
    for (key, script) in &current_scripts {
        if !saved_scripts.contains_key(key) {
            findings.push(DependencyFinding {
                kind: DependencyKind::Script,
                identity: script.name.clone(),
                affects: SavedStateKind::Vm,
                change: DependencyChange::Added,
            });
        }
    }
    findings.sort();
    DependencyReport {
        source: DependencySource::Native,
        findings,
    }
}

fn vanilla_dependency_report(
    plugins: &[soe_save::WString],
    light_plugins: &[soe_save::WString],
    papyrus: Option<&Papyrus>,
    current: &SaveProvenance,
) -> DependencyReport {
    let current_plugins: std::collections::BTreeMap<_, _> = current
        .plugins
        .iter()
        .map(|plugin| (plugin.name.to_ascii_lowercase(), plugin))
        .collect();
    let mut findings = Vec::new();
    let mut saved_plugins = std::collections::BTreeMap::new();
    for (names, light) in [(plugins, false), (light_plugins, true)] {
        for entry in names {
            let identity = entry.to_text();
            let key = identity.to_ascii_lowercase();
            if saved_plugins
                .insert(key.clone(), (identity.clone(), light))
                .is_some()
            {
                continue;
            }
            match current_plugins.get(&key) {
                None => findings.push(DependencyFinding {
                    kind: DependencyKind::Plugin,
                    identity: identity.clone(),
                    affects: SavedStateKind::World,
                    change: DependencyChange::Missing,
                }),
                Some(now) if now.light != light => findings.push(DependencyFinding {
                    kind: DependencyKind::Plugin,
                    identity: identity.clone(),
                    affects: SavedStateKind::World,
                    change: DependencyChange::KindChanged,
                }),
                Some(_) => {}
            }
            findings.push(DependencyFinding {
                kind: DependencyKind::Plugin,
                identity,
                affects: SavedStateKind::World,
                change: DependencyChange::ProvenanceUnknown,
            });
        }
    }
    if let Some(papyrus) = papyrus {
        let mut saved_scripts = std::collections::BTreeSet::new();
        for script in &papyrus.scripts {
            if let Some(name) = papyrus.string(script.name) {
                saved_scripts.insert(name.to_owned());
            }
            if let Some(parent) = papyrus.string(script.parent) {
                if !parent.is_empty() {
                    saved_scripts.insert(parent.to_owned());
                }
            }
        }
        for instance in &papyrus.instances {
            if let Some(name) = papyrus.string(instance.script) {
                saved_scripts.insert(name.to_owned());
            }
        }
        for reference in &papyrus.references {
            if let Some(name) = papyrus.string(reference.script) {
                saved_scripts.insert(name.to_owned());
            }
        }
        for active in &papyrus.execution.active_data {
            for frame in &active.frames {
                if let Some(name) = papyrus.string(frame.script_name) {
                    saved_scripts.insert(name.to_owned());
                }
            }
        }
        for message in &papyrus.execution.messages {
            if let Some(data) = &message.message {
                if let Some(name) = papyrus.string(data.script_name) {
                    saved_scripts.insert(name.to_owned());
                }
            }
        }
        for stack in papyrus.execution.suspended.iter().flatten() {
            if let Some(data) = &stack.message {
                if let Some(name) = papyrus.string(data.script_name) {
                    saved_scripts.insert(name.to_owned());
                }
            }
        }
        let current_scripts: std::collections::BTreeSet<_> = current
            .scripts
            .iter()
            .map(|script| script.name.to_ascii_lowercase())
            .collect();
        for name in saved_scripts {
            if !current_scripts.contains(&name.to_ascii_lowercase()) {
                findings.push(DependencyFinding {
                    kind: DependencyKind::Script,
                    identity: name.clone(),
                    affects: SavedStateKind::Vm,
                    change: DependencyChange::Missing,
                });
            }
            findings.push(DependencyFinding {
                kind: DependencyKind::Script,
                identity: name,
                affects: SavedStateKind::Vm,
                change: DependencyChange::ProvenanceUnknown,
            });
        }
    }
    findings.sort();
    DependencyReport {
        source: DependencySource::Vanilla,
        findings,
    }
}

impl Game {
    /// Serialize the world's run-time state and the VM's.
    ///
    /// # Errors
    ///
    /// Whatever [`soe_papyrus::Vm::save_state`] returns for a thread it
    /// cannot describe (a frame with no script).
    pub fn save_state(&self) -> Result<Vec<u8>, VmError> {
        let _span = soe_trace::Span::new("game.save_state", "game");
        let world_bytes = self.world.borrow().save_state();
        let vm_bytes = self.vm.save_state()?;
        let mut out = Writer::new();
        out.out.extend_from_slice(MAGIC);
        out.u16(VERSION);
        current_provenance(self).write(&mut out);
        out.bytes(&world_bytes);
        out.bytes(&vm_bytes);
        Ok(out.out)
    }

    /// Replace the world and VM state with a saved one. Compatible scripts
    /// should be loaded first. Missing or changed Papyrus definitions suspend
    /// the saved VM state until compatible scripts load. The game should not
    /// have been bound or started, `load_state` overwrites object and instance
    /// state wholesale rather than merging it.
    ///
    /// # Errors
    ///
    /// A wrong magic or version, a truncated stream, or anything
    /// `World::load_state` or `Vm::load_state` reject.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let _span = soe_trace::Span::with_payload("game.load_state", "game", bytes.len() as u64);
        match self.validate_saved_state(bytes) {
            Ok(())
            | Err(
                VmError::UnknownScript(_)
                | VmError::UnknownFunction { .. }
                | VmError::IncompatibleScript(_),
            ) => {}
            Err(error) => return Err(error),
        }
        let (saved, world_bytes, vm_bytes) = read_envelope(bytes)?;
        check_provenance(&saved, &current_provenance(self))?;
        self.world.borrow_mut().load_state(world_bytes)?;
        self.vm.load_state(vm_bytes)?;
        self.bound = None;
        self.bound_cells.clear();
        self.bound_forms.clear();
        Ok(())
    }
}

impl Game {
    /// Serialize and atomically replace a native save after read-back validation.
    ///
    /// # Errors
    /// State serialization, validation or a file persistence operation fails.
    pub fn write_file(&self, path: &std::path::Path) -> Result<(), soe_save::atomic::WriteError> {
        let bytes = self
            .save_state()
            .map_err(|error| soe_save::atomic::WriteError::Serialize(error.to_string()))?;
        self.write_state_file(path, &bytes)
    }

    /// Write previously serialized native bytes against this game's definitions.
    /// Validation uses scratch world and VM state and executes no script callbacks.
    ///
    /// # Errors
    /// Invalid native bytes, missing definitions or file persistence failures.
    pub fn write_state_file(
        &self,
        path: &std::path::Path,
        bytes: &[u8],
    ) -> Result<(), soe_save::atomic::WriteError> {
        soe_save::atomic::write_validated(path, bytes, |written| {
            self.validate_saved_state(written)
                .map_err(|error| error.to_string())
        })
    }

    pub(crate) fn validate_saved_state(&self, bytes: &[u8]) -> Result<(), VmError> {
        let (saved, world, vm) = read_envelope(bytes)?;
        check_provenance(&saved, &current_provenance(self))?;
        self.world.borrow().validate_state(world)?;
        self.vm.validate_state(vm)
    }

    /// Return the dependency evidence recorded in a native save.
    pub fn saved_provenance(bytes: &[u8]) -> Result<SaveProvenance, VmError> {
        read_envelope(bytes).map(|(provenance, _, _)| provenance)
    }

    /// Compare native save dependencies with this game's definitions.
    /// The report is read-only and lists every difference in stable order.
    pub fn preflight_dependencies(&self, bytes: &[u8]) -> Result<DependencyReport, VmError> {
        let (saved, _, _) = read_envelope(bytes)?;
        Ok(native_dependency_report(&saved, &current_provenance(self)))
    }

    /// Compare vanilla save dependencies with this game's definitions.
    /// ESS does not store content fingerprints, so every listed plugin and
    /// script is reported with unknown provenance. Papyrus parsing errors are
    /// returned before any saved state can be applied.
    pub fn preflight_vanilla_dependencies(&self, save: &Save) -> Result<DependencyReport, VmError> {
        let papyrus = save
            .papyrus()
            .transpose()
            .map_err(|error| VmError::State(format!("invalid saved Papyrus data: {error}")))?;
        Ok(vanilla_dependency_report(
            &save.plugins,
            &save.light_plugins,
            papyrus.as_ref(),
            &current_provenance(self),
        ))
    }
}
