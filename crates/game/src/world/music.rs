// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient active music types controlled by Papyrus.

use soe_esm::{Error, GlobalFormId, Result, Tag};

use super::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

impl World {
    /// Active music types in the order they were added.
    #[must_use]
    pub fn active_music_types(&self) -> &[GlobalFormId] {
        &self.music_types
    }

    /// Add a live MUSC record to the active music type list.
    ///
    /// Returns `true` when the list changed.
    ///
    /// # Errors
    /// Returns an error when `id` does not resolve to a live MUSC record.
    pub fn add_music_type(&mut self, id: GlobalFormId) -> Result<bool> {
        self.validate_music_type(id)?;
        if self.music_types.contains(&id) {
            return Ok(false);
        }
        self.music_types.push(id);
        Ok(true)
    }

    /// Remove a live MUSC record from the active music type list.
    ///
    /// Returns `true` when the list changed.
    ///
    /// # Errors
    /// Returns an error when `id` does not resolve to a live MUSC record.
    pub fn remove_music_type(&mut self, id: GlobalFormId) -> Result<bool> {
        self.validate_music_type(id)?;
        let Some(index) = self.music_types.iter().position(|active| *active == id) else {
            return Ok(false);
        };
        self.music_types.remove(index);
        Ok(true)
    }

    fn validate_music_type(&self, id: GlobalFormId) -> Result<()> {
        let record = self
            .order
            .winner_ref(id)
            .map(|owner| self.order.record(owner))
            .ok_or_else(|| Error::Invalid(format!("missing MUSC {id}")))?;
        if record.tag != T(b"MUSC") || record.is_deleted() {
            return Err(Error::Invalid(format!("expected live MUSC at {id}")));
        }
        Ok(())
    }
}
