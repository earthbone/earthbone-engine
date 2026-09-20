// SPDX-License-Identifier: GPL-3.0-or-later

//! Extra-data layouts from UESP Extra_data, xEdit wbChangedExtraData and
//! FallrimTools ChangeFormExtraDataData. Unknown layouts stop decoding.

use super::fields::{array, fields, invalid, list, write, Field};
use crate::cursor::{Cursor, VsVal};
use crate::{RefId, Result};

/// Extra fields in file order. The tag retains the original extra-data type.
#[derive(Clone, Debug)]
pub struct Extra {
    pub kind: u8,
    pub fields: Vec<Field>,
}

/// One stack's or reference's extra-data list.
#[derive(Clone, Debug)]
pub struct ExtraData {
    count: VsVal,
    pub entries: Vec<Extra>,
}

impl ExtraData {
    pub(super) fn read(r: &mut Cursor<'_>) -> Result<Self> {
        let count = r.vsval()?;
        if count.value() as usize > r.remaining() {
            return Err(invalid("extra-data count exceeds remaining bytes"));
        }
        let mut entries = Vec::with_capacity(count.value() as usize);
        for _ in 0..count.value() {
            entries.push(Extra::read(r)?);
        }
        Ok(Self { count, entries })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.count.value() as usize != self.entries.len() {
            return Err(invalid("extra-data count mismatch"));
        }
        self.count.write(out);
        for extra in &self.entries {
            out.push(extra.kind);
            write(&extra.fields, out)?;
        }
        Ok(())
    }
}

fn reference_is_null(id: RefId) -> bool {
    id.value() == 0
}

impl Extra {
    pub(super) fn read(r: &mut Cursor<'_>) -> Result<Self> {
        let at = r.pos();
        let kind = r.u8()?;
        let fields = match kind {
            0 | 22 | 23 | 29 | 32 | 53 | 61 => vec![],
            12 => {
                let mut v = fields(r, "r")?;
                v.push(list(r, "wrww")?);
                let flag = r.u8()?;
                v.push(Field::Byte(flag));
                if flag != 0 {
                    v.extend(fields(r, "ww")?);
                }
                v.extend(fields(r, "b")?);
                v
            }
            24 => fields(r, "rfffw")?,
            25 => fields(r, "rrwbbb")?,
            26 => {
                let id = r.ref_id()?;
                if !reference_is_null(id) {
                    return Err(invalid("non-null TresPassPackage layout is undocumented"));
                }
                vec![Field::Reference(id)]
            }
            27 => vec![list(r, "rb")?],
            28 | 33 | 34 | 56 | 69 | 72 | 101 | 104 | 112 | 133 | 142 | 146 | 157 | 164 => {
                fields(r, "r")?
            }
            30 | 35 | 39 | 83 | 93 | 160 => fields(r, "w")?,
            31 | 44 | 73 | 77 | 84 | 150 | 156 => fields(r, "b")?,
            36 | 79 => fields(r, "s")?,
            37 | 40 | 47 | 85 | 89 => fields(r, "f")?,
            42 => fields(r, "bbrww")?,
            43 => fields(r, "ffffffbr")?,
            45 => {
                let mut v = fields(r, "rr")?;
                let flags = r.u32()?;
                v.push(Field::Word(flags));
                v.push(Field::Group(super::npc::read(r, flags)?));
                v
            }
            46 => fields(r, "wb")?,
            49 => fields(r, "wrwwrfrr")?,
            50 => {
                let mut v = fields(r, "r")?;
                v.push(Field::Array(array(r, false, 6, |r| {
                    let mut target = fields(r, "rb")?;
                    // CONTEXT: This vsval is a scalar, so retain its raw encoding.
                    let scalar = r.vsval()?;
                    let mut bytes = vec![];
                    scalar.write(&mut bytes);
                    target.push(Field::Bytes(bytes));
                    target.push(list(r, "b")?);
                    Ok(target)
                })?));
                v
            }
            52 => vec![list(r, "ww")?],
            62 | 88 | 106 | 149 => fields(r, "rw")?,
            68 => vec![list(r, "f")?],
            76 => fields(r, "tbbbbbrrrr")?,
            91 => {
                let mut v = vec![list(r, "rb")?];
                v.extend(fields(r, "rb")?);
                v
            }
            92 => {
                let mut v = fields(r, "swwbr")?;
                v.push(Field::Array(array(r, false, 5, |r| {
                    let mut limb = fields(r, "w")?;
                    limb.push(list(r, "r")?);
                    Ok(limb)
                })?));
                v
            }
            102 => fields(r, "rr")?,
            108 => {
                let flag = r.u8()?;
                if flag != 0xFF {
                    return Err(invalid(format!(
                        "unsupported PackageData kind {flag} at {at}"
                    )));
                }
                vec![Field::Byte(flag)]
            }
            111 => vec![list(r, "rww")?],
            113 => {
                let mut v = fields(r, "rbwr")?;
                v.push(Field::Array(array(r, false, 23, |r| {
                    fields(r, "ttwwbrrrb")
                })?));
                v.extend(fields(r, "srrrr")?);
                v
            }
            120 => vec![list(r, "rwb")?],
            135 => {
                let mut v = fields(r, "fffrw")?;
                v.push(list(r, "fffrfffrb")?);
                v
            }
            136 => vec![list(r, "rw")?],
            140 => vec![list(r, "r")?],
            152 => {
                let mut v = vec![Field::Array(array(r, false, 3, |r| {
                    let id = r.ref_id()?;
                    let mut arrow = vec![Field::Reference(id)];
                    if !reference_is_null(id) {
                        let flag = r.u16()?;
                        arrow.push(Field::Short(flag));
                        if flag != 0xFFFF {
                            arrow.extend(fields(r, "wffffffff")?);
                        }
                    }
                    Ok(arrow)
                })?)];
                v.extend(fields(r, "ss")?);
                v
            }
            153 => {
                let first = r.ref_id()?;
                let second = r.ref_id()?;
                let mode = r.u32()?;
                let mut v = vec![
                    Field::Reference(first),
                    Field::Reference(second),
                    Field::Word(mode),
                ];
                if reference_is_null(first) && reference_is_null(second) && mode == 0xFFFFFFFE {
                    v.extend(fields(r, "t")?);
                }
                v
            }
            155 => fields(r, "rs")?,
            159 => fields(r, "ws")?,
            161 => fields(r, "ffffffffffffffffffwwww")?,
            169 => fields(r, "wrrb")?,
            174 => fields(r, "wrttffffffwf")?,
            175 => vec![Field::Array(array(r, true, 7, |r| fields(r, "rw"))?)],
            176 => fields(r, "ff")?,
            _ => return Err(invalid(format!("unknown extra-data type {kind} at {at}"))),
        };
        Ok(Self { kind, fields })
    }
}
