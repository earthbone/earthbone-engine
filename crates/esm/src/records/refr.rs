// SPDX-License-Identifier: GPL-3.0-or-later

//! REFR: a placed instance of a base object in a cell.

use crate::cursor::Fields;
use crate::error::Result;
use crate::plugin::{flags as record_flags, Record};
use crate::tag::{self, Tag};

use super::{f32_field, form_id, Transform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnableParent {
    pub parent: u32,
    /// Bit 0: set enable state to opposite of parent. Bit 1: pop in.
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Teleport {
    pub door: u32,
    pub destination: Transform,
    pub flags: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkedRef {
    pub keyword: u32,
    pub reference: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Refr {
    pub form_id: u32,
    pub record_flags: u32,
    pub editor_id: Option<String>,
    /// NAME: the base object this places.
    pub base: u32,
    pub transform: Option<Transform>,
    /// XSCL, defaulting to one.
    pub scale: f32,
    pub enable_parent: Option<EnableParent>,
    pub owner: Option<u32>,
    pub count: Option<i32>,
    pub teleport: Option<Teleport>,
    pub location: Option<u32>,
    pub layer: Option<u32>,
    pub encounter_zone: Option<u32>,
    pub emittance: Option<u32>,
    pub linked_refs: Vec<LinkedRef>,
    /// XPRM as stored: a primitive's bounds, colour and kind.
    pub primitive: Option<Vec<u8>>,
    pub cell: Option<u32>,
    pub world: Option<u32>,
    /// From the group tree: in the cell's persistent children group.
    pub in_persistent_group: bool,
}

impl Refr {
    pub fn from_record(rec: &Record) -> Result<Refr> {
        debug_assert_eq!(rec.tag, tag::REFR);
        let base = {
            let d = rec.required(Tag::new(b"NAME"))?;
            let mut f = Fields::new(rec.tag, Tag::new(b"NAME"), d);
            f.exact(4)?;
            f.u32()?
        };
        let transform = match rec.find(Tag::new(b"DATA")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"DATA"), d);
                f.exact(24)?;
                Some(Transform::read(&mut f)?)
            }
        };
        let enable_parent = match rec.find(Tag::new(b"XESP")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"XESP"), d);
                f.exact(8)?;
                Some(EnableParent {
                    parent: f.u32()?,
                    flags: f.u32()?,
                })
            }
        };
        let teleport = match rec.find(Tag::new(b"XTEL")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"XTEL"), d);
                if d.len() != 28 && d.len() != 32 {
                    f.exact(32)?;
                }
                let door = f.u32()?;
                let destination = Transform::read(&mut f)?;
                let flags = if f.remaining() >= 4 {
                    Some(f.u32()?)
                } else {
                    None
                };
                Some(Teleport {
                    door,
                    destination,
                    flags,
                })
            }
        };
        let count = match rec.find(Tag::new(b"XCNT")) {
            None => None,
            Some(d) => {
                let mut f = Fields::new(rec.tag, Tag::new(b"XCNT"), d);
                f.exact(4)?;
                Some(f.i32()?)
            }
        };
        let mut linked_refs = Vec::new();
        for d in rec.all(Tag::new(b"XLKR")) {
            let mut f = Fields::new(rec.tag, Tag::new(b"XLKR"), d);
            // Older files wrote only the reference.
            match d.len() {
                4 => linked_refs.push(LinkedRef {
                    keyword: 0,
                    reference: f.u32()?,
                }),
                _ => {
                    f.exact(8)?;
                    linked_refs.push(LinkedRef {
                        keyword: f.u32()?,
                        reference: f.u32()?,
                    });
                }
            }
        }
        Ok(Refr {
            form_id: rec.form_id,
            record_flags: rec.flags,
            editor_id: rec.editor_id(),
            base,
            transform,
            scale: f32_field(rec, Tag::new(b"XSCL"))?.unwrap_or(1.0),
            enable_parent,
            owner: form_id(rec, Tag::new(b"XOWN"))?,
            count,
            teleport,
            location: form_id(rec, Tag::new(b"XLCN"))?,
            layer: form_id(rec, Tag::new(b"XLYR"))?,
            encounter_zone: form_id(rec, Tag::new(b"XEZN"))?,
            emittance: form_id(rec, Tag::new(b"XEMI"))?,
            linked_refs,
            primitive: rec.find(Tag::new(b"XPRM")).map(<[u8]>::to_vec),
            cell: rec.placement.cell,
            world: rec.placement.world,
            in_persistent_group: matches!(
                rec.placement.group,
                Some(crate::plugin::GroupKind::CellPersistent(_))
            ),
        })
    }

    pub fn is_persistent(&self) -> bool {
        self.record_flags & record_flags::PERSISTENT != 0
    }

    pub fn is_initially_disabled(&self) -> bool {
        self.record_flags & record_flags::INITIALLY_DISABLED != 0
    }

    pub fn is_deleted(&self) -> bool {
        self.record_flags & record_flags::DELETED != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placed_door_reads_its_transform_and_teleport() {
        let t = Transform {
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.5, 1.5],
        };
        let mut xtel = 0x2000u32.to_le_bytes().to_vec();
        xtel.extend_from_slice(&t.to_bytes());
        xtel.extend_from_slice(&1u32.to_le_bytes());
        let rec = Record::new(tag::REFR, 0x3000)
            .with_flags(record_flags::PERSISTENT)
            .sub(b"NAME", 0x1000u32.to_le_bytes())
            .sub(b"DATA", t.to_bytes())
            .sub(b"XSCL", 2.5f32.to_le_bytes())
            .sub(b"XTEL", xtel)
            .sub(
                b"XESP",
                [0x40u32.to_le_bytes(), 1u32.to_le_bytes()].concat(),
            )
            .sub(
                b"XLKR",
                [0x50u32.to_le_bytes(), 0x60u32.to_le_bytes()].concat(),
            );
        let r = Refr::from_record(&rec).unwrap();
        assert_eq!(r.base, 0x1000);
        assert_eq!(r.transform, Some(t));
        assert_eq!(r.scale, 2.5);
        assert!(r.is_persistent());
        assert_eq!(r.teleport.unwrap().door, 0x2000);
        assert_eq!(r.teleport.unwrap().destination, t);
        assert_eq!(
            r.enable_parent,
            Some(EnableParent {
                parent: 0x40,
                flags: 1
            })
        );
        assert_eq!(
            r.linked_refs,
            vec![LinkedRef {
                keyword: 0x50,
                reference: 0x60
            }]
        );
    }

    #[test]
    fn a_reference_without_a_base_is_an_error() {
        assert!(Refr::from_record(&Record::new(tag::REFR, 1)).is_err());
        let bad = Record::new(tag::REFR, 1).sub(b"NAME", [1, 2]);
        assert!(Refr::from_record(&bad).is_err());
        let short_data = Record::new(tag::REFR, 1)
            .sub(b"NAME", 1u32.to_le_bytes())
            .sub(b"DATA", [0u8; 20]);
        assert!(Refr::from_record(&short_data).is_err());
    }
}
