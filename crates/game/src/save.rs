// SPDX-License-Identifier: GPL-3.0-or-later

//! Save and load for a whole [`Game`]: the world's run-time state and the
//! VM's, one after the other inside a small envelope of their own.
//!
//! Plugin and loaded-script fingerprints travel in the file as provenance.
//! The caller still supplies those definitions before `load_state`. The
//! loader rejects changed dependencies and slot movement before replacing
//! world or VM state.

use soe_esm::Slot;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

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
    if saved.scripts != current.scripts {
        return Err(VmError::State(
            "loaded script fingerprints differ from the save".into(),
        ));
    }
    Ok(())
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

    /// Replace the world and VM state with a saved one. The `Game` must
    /// already carry the same scripts the save was written with, and
    /// should not have been bound or started, `load_state` overwrites
    /// object and instance state wholesale rather than merging it.
    ///
    /// # Errors
    ///
    /// A wrong magic or version, a truncated stream, or anything
    /// `World::load_state` or `Vm::load_state` reject.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let _span = soe_trace::Span::with_payload("game.load_state", "game", bytes.len() as u64);
        let (saved, world_bytes, vm_bytes) = read_envelope(bytes)?;
        check_provenance(&saved, &current_provenance(self))?;
        self.world.borrow_mut().load_state(world_bytes)?;
        self.vm.load_state(vm_bytes)?;
        self.bound = None;
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
}
