// SPDX-License-Identifier: GPL-3.0-or-later

//! REFR/ACHR and projectile placement headers from xEdit wbDefinitionsTES5Saves.pas,
//! UESP and FallrimTools ChangeFormInitialData. Extra data, inventory and
//! actor execution follow this header and are not consumed by this reader.

mod contents;
mod extra;
mod fields;
mod leveled;
mod npc;
mod package;
mod player;
pub use contents::{Inventory, InventoryEntry, ReferenceContents};
pub use extra::{Extra, ExtraData};
pub use fields::{Field, FieldArray};
pub use leveled::LeveledCreature;
pub use player::PlayerLocation;

use crate::cursor::{Cursor, VsVal};
use crate::{ChangeForm, Error, RefId, RefKind, Result};

/// Common reference header bits. Remaining bits describe later sections.
pub mod flags {
    pub const FORM: u32 = 1;
    pub const MOVE: u32 = 1 << 1;
    pub const HAVOK: u32 = 1 << 2;
    pub const CELL: u32 = 1 << 3;
    pub const SCALE: u32 = 1 << 4;
    pub const BASE: u32 = 1 << 7;
    pub const PROMOTED: u32 = 1 << 25;
}

/// Saved position and Euler rotation in Creation Engine coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub cell_or_world: RefId,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

/// The discriminator comes from the change-form identity and flags.
#[derive(Clone, Debug, PartialEq)]
pub enum Initial {
    Unchanged,
    Moved(Placement),
    Created {
        placement: Placement,
        flags: u8,
        base: RefId,
    },
    Promoted {
        placement: Placement,
        secondary_cell: RefId,
        coordinates: [i16; 2],
    },
}

impl Initial {
    /// Position shared by all nonempty initial layouts.
    pub fn placement(&self) -> Option<&Placement> {
        match self {
            Self::Unchanged => None,
            Self::Moved(value)
            | Self::Created {
                placement: value, ..
            }
            | Self::Promoted {
                placement: value, ..
            } => Some(value),
        }
    }
}

/// Scale format 55 alone uses an integer percentage, per xEdit's decider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    Float(f32),
    Percent(u16),
}

impl Scale {
    /// CommonLib stores reference scale as a percentage of one.
    pub fn factor(self) -> f32 {
        match self {
            Self::Float(value) => value,
            Self::Percent(value) => f32::from(value) / 100.0,
        }
    }
}

/// An opaque Havok block with its original variable-count width.
#[derive(Clone, Debug)]
pub struct Havok {
    count: VsVal,
    pub data: Vec<u8>,
}

/// Fully bounded placement prefix, with an explicit unconsumed remainder.
#[derive(Clone, Debug)]
pub struct ReferenceHeader {
    pub change_flags: u32,
    pub initial: Initial,
    /// Whether this change-form type carries FORM, BASE and SCALE fields.
    pub carries_reference_data: bool,
    /// Whether this change-form type carries the generic Havok block.
    pub carries_havok: bool,
    pub havok: Option<Havok>,
    /// xEdit calls these HasEquipData and flags1. Their consumers are separate.
    pub actor: Option<[u32; 2]>,
    /// xEdit names these Mask Invert Flags and Mask Set unk010.
    pub form_flags: Option<(u32, u16)>,
    pub base: Option<RefId>,
    pub scale: Option<Scale>,
    pub bytes_consumed: usize,
    pub remaining_bytes: usize,
}

fn placement(r: &mut Cursor<'_>) -> Result<Placement> {
    Ok(Placement {
        cell_or_world: r.ref_id()?,
        position: [r.f32()?, r.f32()?, r.f32()?],
        rotation: [r.f32()?, r.f32()?, r.f32()?],
    })
}

impl ChangeForm {
    /// Read the placement prefix of a REFR, ACHR, PMIS, PGRE, PBEA or PCON payload.
    ///
    /// # Errors
    /// A malformed compressed payload, invalid identity tag, truncated prefix
    /// or a Havok count exceeding the payload. The reported remainder is not
    /// validated, decoded or implicitly considered restored.
    pub fn reference_header(&self, form_version: u8) -> Result<Option<ReferenceHeader>> {
        if !matches!(self.type_index(), 0 | 1 | 2 | 3 | 4 | 42 | 47) {
            return Ok(None);
        }
        if self.form.kind() == RefKind::Invalid {
            return Err(Error::Reference {
                reason: "invalid reference identity tag".into(),
            });
        }
        let payload = self.payload()?;
        let mut r = Cursor::new(&payload, "reference header");
        let flags = self.change_flags;
        let initial = if self.type_index() == 47 {
            // xEdit has no InitialDataTypeDecider entry for PARW. Its entire
            // payload remains opaque, regardless of the change flags.
            Initial::Unchanged
        } else if self.form.kind() == RefKind::Created {
            Initial::Created {
                placement: placement(&mut r)?,
                flags: r.u8()?,
                base: r.ref_id()?,
            }
        } else if flags & (flags::PROMOTED | flags::CELL) != 0 {
            Initial::Promoted {
                placement: placement(&mut r)?,
                secondary_cell: r.ref_id()?,
                coordinates: [r.i16()?, r.i16()?],
            }
        } else if flags & (flags::MOVE | flags::HAVOK) != 0 {
            Initial::Moved(placement(&mut r)?)
        } else {
            Initial::Unchanged
        };
        let carries_havok = self.type_index() != 47;
        let havok = if carries_havok && flags & flags::HAVOK != 0 {
            let count = r.vsval()?;
            Some(Havok {
                data: r.take(count.value() as usize)?.to_vec(),
                count,
            })
        } else {
            None
        };
        let actor = if self.type_index() == 1 {
            Some([r.u32()?, r.u32()?])
        } else {
            None
        };
        // CONTEXT: xEdit binds PCON to initial placement data but leaves its
        // subtype struct empty and omits PARW from InitialDataTypeDecider.
        let has_reference_data = matches!(self.type_index(), 0 | 1);
        let form_flags = if has_reference_data && flags & flags::FORM != 0 {
            Some((r.u32()?, r.u16()?))
        } else {
            None
        };
        let base = if has_reference_data && flags & flags::BASE != 0 {
            Some(r.ref_id()?)
        } else {
            None
        };
        let scale = if has_reference_data && flags & flags::SCALE != 0 {
            Some(if form_version == 55 {
                Scale::Percent(r.u16()?)
            } else {
                Scale::Float(r.f32()?)
            })
        } else {
            None
        };
        Ok(Some(ReferenceHeader {
            change_flags: flags,
            initial,
            carries_reference_data: has_reference_data,
            carries_havok,
            havok,
            actor,
            form_flags,
            base,
            scale,
            bytes_consumed: r.pos(),
            remaining_bytes: r.remaining(),
        }))
    }
}

fn write_placement(out: &mut Vec<u8>, value: &Placement) {
    value.cell_or_world.write(out);
    for number in value.position.iter().chain(&value.rotation) {
        out.extend_from_slice(&number.to_le_bytes());
    }
}

impl ReferenceHeader {
    /// Rebuild only the placement prefix from its typed fields.
    ///
    /// # Errors
    /// An optional field disagrees with its change bit or a retained count
    /// no longer fits the edited Havok data. This does not encode the suffix.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut fields = vec![];
        if self.carries_havok {
            fields.push((flags::HAVOK, self.havok.is_some()));
        }
        if self.carries_reference_data {
            fields.extend([
                (flags::FORM, self.form_flags.is_some()),
                (flags::BASE, self.base.is_some()),
                (flags::SCALE, self.scale.is_some()),
            ]);
        }
        for (flag, present) in fields {
            if (self.change_flags & flag != 0) != present {
                return Err(Error::Reference {
                    reason: "header field presence mismatch".into(),
                });
            }
        }
        let mut out = vec![];
        match &self.initial {
            Initial::Unchanged => {}
            Initial::Moved(value) => write_placement(&mut out, value),
            Initial::Created {
                placement,
                flags,
                base,
            } => {
                write_placement(&mut out, placement);
                out.push(*flags);
                base.write(&mut out);
            }
            Initial::Promoted {
                placement,
                secondary_cell,
                coordinates,
            } => {
                write_placement(&mut out, placement);
                secondary_cell.write(&mut out);
                for coordinate in coordinates {
                    out.extend_from_slice(&coordinate.to_le_bytes());
                }
            }
        }
        if let Some(havok) = &self.havok {
            if havok.count.value() as usize != havok.data.len() {
                return Err(Error::Reference {
                    reason: "Havok byte count mismatch".into(),
                });
            }
            havok.count.write(&mut out);
            out.extend_from_slice(&havok.data);
        }
        if let Some(actor) = self.actor {
            for value in actor {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        if let Some((flags, unknown)) = self.form_flags {
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&unknown.to_le_bytes());
        }
        if let Some(base) = self.base {
            base.write(&mut out);
        }
        match self.scale {
            Some(Scale::Float(value)) => out.extend_from_slice(&value.to_le_bytes()),
            Some(Scale::Percent(value)) => out.extend_from_slice(&value.to_le_bytes()),
            None => {}
        }
        Ok(out)
    }
}
