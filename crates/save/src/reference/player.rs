// SPDX-License-Identifier: GPL-3.0-or-later

//! Player location and allocation counter from UESP Player_Location and
//! xEdit's Player Location Struct. Skyrim SE includes the final byte.

use crate::cursor::Cursor;
use crate::{Error, RefId, Result, Save};

/// The independent player position used during world reconstruction.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerLocation {
    pub next_object_id: u32,
    pub world: RefId,
    pub coordinates: [i32; 2],
    pub cell_or_world: RefId,
    pub position: [f32; 3],
    pub unknown: u8,
}

impl PlayerLocation {
    /// Read exactly one Special Edition type-1 payload.
    ///
    /// # Errors
    /// Truncation or extra bytes.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(bytes, "player location");
        let value = Self {
            next_object_id: r.u32()?,
            world: r.ref_id()?,
            coordinates: [r.u32()? as i32, r.u32()? as i32],
            cell_or_world: r.ref_id()?,
            position: [r.f32()?, r.f32()?, r.f32()?],
            unknown: r.u8()?,
        };
        if !r.is_empty() {
            return Err(Error::Reference {
                reason: "trailing player location bytes".into(),
            });
        }
        Ok(value)
    }

    /// Rebuild the payload from fields, preserving floating-point bits.
    pub fn data_bytes(&self) -> Vec<u8> {
        let mut out = vec![];
        out.extend_from_slice(&self.next_object_id.to_le_bytes());
        self.world.write(&mut out);
        for coordinate in self.coordinates {
            out.extend_from_slice(&coordinate.to_le_bytes());
        }
        self.cell_or_world.write(&mut out);
        for value in self.position {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.push(self.unknown);
        out
    }
}

impl Save {
    /// Decode the optional type-1 player location block.
    ///
    /// # Errors
    /// Duplicate blocks or an invalid payload.
    pub fn player_location(&self) -> Result<Option<PlayerLocation>> {
        let mut blocks = self.table_1.iter().filter(|block| block.kind == 1);
        let first = blocks.next();
        if blocks.next().is_some() {
            return Err(Error::Reference {
                reason: "duplicate player location block".into(),
            });
        }
        first
            .map(|block| PlayerLocation::read(&block.data))
            .transpose()
    }
}
