// SPDX-License-Identifier: GPL-3.0-or-later

//! Save and load for a whole [`Game`]: the world's run-time state and the
//! VM's, one after the other inside a small envelope of their own.
//!
//! Neither the load order nor the loaded scripts travel in the file. Both
//! are the caller's to set up first, the same way [`Game::new`] and
//! [`Game::load_script`] already ask for them, so `load_state` expects a
//! `Game` built against the same load order with the same scripts loaded
//! (and not yet bound or started) before it runs. `World::load_state` and
//! [`soe_papyrus::Vm::load_state`] catch what they can, an unknown script
//! or a quest missing from the load order, but a save read back against a
//! mismatched load order can still succeed with the wrong forms.

use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

use crate::Game;

pub const MAGIC: &[u8; 4] = b"SOEG";
pub const VERSION: u16 = 1;

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
        let mut r = Reader::new(bytes);
        if r.take(4)? != MAGIC {
            return Err(VmError::State("not a game save file".into()));
        }
        let version = r.u16()?;
        if version != VERSION {
            return Err(VmError::State(format!(
                "game save version {version}, this build reads {VERSION}"
            )));
        }
        let world_bytes = r.bytes()?.to_vec();
        let vm_bytes = r.bytes()?.to_vec();
        r.expect_end()?;
        self.world.borrow_mut().load_state(&world_bytes)?;
        self.vm.load_state(&vm_bytes)?;
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
        let mut reader = Reader::new(bytes);
        if reader.take(4)? != MAGIC || reader.u16()? != VERSION {
            return Err(VmError::State("unsupported native save header".into()));
        }
        let world = reader.bytes()?;
        let vm = reader.bytes()?;
        reader.expect_end()?;
        self.world.borrow().validate_state(world)?;
        self.vm.validate_state(vm)
    }
}
