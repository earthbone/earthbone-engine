// SPDX-License-Identifier: GPL-3.0-or-later

//! SCR-40 reference flags and the saved cleared-destruction marker.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use super::{save::FormReader, save_sections::Section, RefState, World};

impl World {
    /// Set whether a placed object is disallowed as a follower favor.
    ///
    /// Returns `false` when `reference` does not resolve to a placed reference.
    pub fn set_no_favor_allowed(&mut self, reference: GlobalFormId, allowed: bool) -> bool {
        let Some(state) = self.reference(reference) else {
            return false;
        };
        state.no_favor_allowed = allowed;
        true
    }
}

fn write(value: &RefState, writer: &mut Writer, _pool: &mut ArrayPool) {
    writer.bool(value.no_favor_allowed);
    writer.bool(value.destruction_cleared);
}

fn read(
    value: &mut RefState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    value.no_favor_allowed = reader.bool()?;
    value.destruction_cleared = reader.bool()?;
    Ok(())
}

pub(super) const SECTION: Section<RefState> = Section {
    name: "SCR-40",
    write,
    read,
};
