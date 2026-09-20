// SPDX-License-Identifier: GPL-3.0-or-later

//! The packfile container: header, section table, class names, fixups, and a
//! bounds-checked view over objects in the data section.
//!
//! A packfile is a memory image. Objects sit in the `__data__` section at
//! their in-memory offsets, with every pointer field zeroed and three fixup
//! tables saying what each pointer should become:
//!
//! - a local fixup maps a field position to another position in the same
//!   section (array contents, strings),
//! - a global fixup maps a field position to an object in some section
//!   (object pointers, including the elements of pointer arrays),
//! - a virtual fixup maps an object position to its class name, which is how
//!   the loader knows which constructor, or here which layout, to use.
//!
//! Pointer width comes from the `hk_2010.2.0-r1` header. An `hkArray<T>` is a null
//! pointer, an `i32` count and an `i32` capacity-and-flags word. An
//! `hkStringPtr` is a pointer. Both are resolved through local fixups.

use std::collections::{BTreeMap, HashMap};

use crate::{Error, QsTransform, Result};

pub const MAGIC0: u32 = 0x57E0_E057;
pub const MAGIC1: u32 = 0x10C0_C010;
pub const HEADER_SIZE: usize = 64;
pub const SECTION_HEADER_SIZE: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub user_tag: i32,
    pub file_version: i32,
    pub pointer_size: u8,
    pub little_endian: bool,
    pub padding_option: u8,
    pub base_class: u8,
    pub section_count: i32,
    pub contents_section_index: i32,
    pub contents_section_offset: i32,
    pub contents_class_name_section_index: i32,
    pub contents_class_name_section_offset: i32,
    pub version_string: String,
    pub flags: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHeader {
    pub tag: String,
    pub absolute_data_start: u32,
    pub local_fixups_offset: u32,
    pub global_fixups_offset: u32,
    pub virtual_fixups_offset: u32,
    pub exports_offset: u32,
    pub imports_offset: u32,
    pub end_offset: u32,
}

/// A parsed packfile. Only the data section is kept as bytes, the class
/// names and fixups are decoded into maps keyed by data-section offset.
#[derive(Debug, Clone)]
pub struct Packfile {
    pub header: Header,
    pub sections: Vec<SectionHeader>,
    /// Class name by its offset in the `__classnames__` section. Both the
    /// signature position and the name position map to the name, since
    /// files differ in which one their virtual fixups point at.
    pub class_names: BTreeMap<u32, String>,
    /// The `__data__` section, offsets in every map below are into this.
    pub data: Vec<u8>,
    pub local_fixups: HashMap<u32, u32>,
    /// Field position to (section index, object offset).
    pub global_fixups: HashMap<u32, (u32, u32)>,
    /// Object offset to class name.
    pub objects: BTreeMap<u32, String>,
    data_section_index: u32,
}

fn le_u32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn le_i32(b: &[u8]) -> i32 {
    i32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn need(what: &'static str, buf: &[u8], at: usize, n: usize) -> Result<()> {
    if at.checked_add(n).is_some_and(|end| end <= buf.len()) {
        Ok(())
    } else {
        Err(Error::Truncated {
            what,
            at,
            need: n,
            have: buf.len().saturating_sub(at),
        })
    }
}

fn c_string(buf: &[u8], at: usize) -> Option<&str> {
    let rest = buf.get(at..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&rest[..end]).ok()
}

pub(crate) fn read_headers(bytes: &[u8]) -> Result<(Header, Vec<SectionHeader>, usize)> {
    need("header", bytes, 0, HEADER_SIZE)?;
    if le_u32(&bytes[0..4]) != MAGIC0 || le_u32(&bytes[4..8]) != MAGIC1 {
        return Err(Error::NotAPackfile);
    }
    let little_endian = bytes[17] == 1;
    if !little_endian {
        return Err(Error::BigEndian);
    }
    let version_end = bytes[40..56]
        .iter()
        .position(|&b| b == 0 || b == 0xFF)
        .unwrap_or(16);
    let header = Header {
        user_tag: le_i32(&bytes[8..12]),
        file_version: le_i32(&bytes[12..16]),
        pointer_size: bytes[16],
        little_endian,
        padding_option: bytes[18],
        base_class: bytes[19],
        section_count: le_i32(&bytes[20..24]),
        contents_section_index: le_i32(&bytes[24..28]),
        contents_section_offset: le_i32(&bytes[28..32]),
        contents_class_name_section_index: le_i32(&bytes[32..36]),
        contents_class_name_section_offset: le_i32(&bytes[36..40]),
        version_string: String::from_utf8_lossy(&bytes[40..40 + version_end]).into_owned(),
        flags: le_i32(&bytes[56..60]),
    };
    if header.pointer_size != 4 && header.pointer_size != 8 {
        return Err(Error::PointerSize(header.pointer_size));
    }
    // The 2010 writer fills section_offset with 0xFFFF, which means zero.
    let section_offset = i16::from_le_bytes([bytes[62], bytes[63]]);
    let table_start = HEADER_SIZE
        + if section_offset < 0 {
            0
        } else {
            section_offset as usize
        };
    if header.section_count < 0 || header.section_count > 64 {
        return Err(Error::Truncated {
            what: "section table",
            at: 20,
            need: 0,
            have: header.section_count as usize,
        });
    }
    need(
        "section table",
        bytes,
        table_start,
        header.section_count as usize * SECTION_HEADER_SIZE,
    )?;
    let mut sections = Vec::with_capacity(header.section_count as usize);
    for i in 0..header.section_count as usize {
        let at = table_start + i * SECTION_HEADER_SIZE;
        need("section header", bytes, at, SECTION_HEADER_SIZE)?;
        let s = &bytes[at..at + SECTION_HEADER_SIZE];
        let tag_end = s[..19].iter().position(|&b| b == 0).unwrap_or(19);
        sections.push(SectionHeader {
            tag: String::from_utf8_lossy(&s[..tag_end]).into_owned(),
            absolute_data_start: le_u32(&s[20..24]),
            local_fixups_offset: le_u32(&s[24..28]),
            global_fixups_offset: le_u32(&s[28..32]),
            virtual_fixups_offset: le_u32(&s[32..36]),
            exports_offset: le_u32(&s[36..40]),
            imports_offset: le_u32(&s[40..44]),
            end_offset: le_u32(&s[44..48]),
        });
    }

    Ok((header, sections, table_start))
}

impl Packfile {
    /// Parse a whole file.
    pub fn parse(bytes: &[u8]) -> Result<Packfile> {
        let (header, sections, _) = read_headers(bytes)?;

        let class_section = sections
            .iter()
            .position(|s| s.tag == "__classnames__")
            .ok_or(Error::MissingSection("__classnames__"))?;
        let data_section = sections
            .iter()
            .position(|s| s.tag == "__data__")
            .ok_or(Error::MissingSection("__data__"))?;

        let class_names = Self::parse_class_names(bytes, &sections[class_section])?;

        let ds = &sections[data_section];
        let start = ds.absolute_data_start as usize;
        if ds.local_fixups_offset > ds.global_fixups_offset
            || ds.global_fixups_offset > ds.virtual_fixups_offset
            || ds.virtual_fixups_offset > ds.exports_offset
            || ds.exports_offset > ds.end_offset
        {
            return Err(Error::Truncated {
                what: "data section fixup table order",
                at: start,
                need: ds.end_offset as usize,
                have: 0,
            });
        }
        need("data section", bytes, start, ds.end_offset as usize)?;
        let data = bytes[start..start + ds.local_fixups_offset as usize].to_vec();

        let mut local_fixups = HashMap::new();
        let mut at = start + ds.local_fixups_offset as usize;
        let end = start + ds.global_fixups_offset as usize;
        while at + 8 <= end {
            let src = le_u32(&bytes[at..]);
            if src == u32::MAX {
                break;
            }
            let dst = le_u32(&bytes[at + 4..]);
            if src as usize + 1 > data.len() || dst as usize > data.len() {
                return Err(Error::BadFixup {
                    kind: "local",
                    src,
                    dst,
                });
            }
            local_fixups.insert(src, dst);
            at += 8;
        }

        let mut global_fixups = HashMap::new();
        let mut at = start + ds.global_fixups_offset as usize;
        let end = start + ds.virtual_fixups_offset as usize;
        while at + 12 <= end {
            let src = le_u32(&bytes[at..]);
            if src == u32::MAX {
                break;
            }
            let section = le_u32(&bytes[at + 4..]);
            let dst = le_u32(&bytes[at + 8..]);
            if src as usize >= data.len() {
                return Err(Error::BadFixup {
                    kind: "global",
                    src,
                    dst,
                });
            }
            if section as usize == data_section && dst as usize >= data.len() {
                return Err(Error::BadFixup {
                    kind: "global",
                    src,
                    dst,
                });
            }
            global_fixups.insert(src, (section, dst));
            at += 12;
        }

        let mut objects = BTreeMap::new();
        let mut at = start + ds.virtual_fixups_offset as usize;
        let end = start + ds.exports_offset as usize;
        while at + 12 <= end {
            let src = le_u32(&bytes[at..]);
            if src == u32::MAX {
                break;
            }
            let name_offset = le_u32(&bytes[at + 8..]);
            if src as usize >= data.len() {
                return Err(Error::BadFixup {
                    kind: "virtual",
                    src,
                    dst: name_offset,
                });
            }
            let name = class_names
                .get(&name_offset)
                .cloned()
                .ok_or(Error::BadFixup {
                    kind: "virtual",
                    src,
                    dst: name_offset,
                })?;
            objects.insert(src, name);
            at += 12;
        }

        Ok(Packfile {
            header,
            sections,
            class_names,
            data,
            local_fixups,
            global_fixups,
            objects,
            data_section_index: data_section as u32,
        })
    }

    fn parse_class_names(bytes: &[u8], section: &SectionHeader) -> Result<BTreeMap<u32, String>> {
        let start = section.absolute_data_start as usize;
        let len = section.local_fixups_offset as usize;
        need("classnames section", bytes, start, len)?;
        let buf = &bytes[start..start + len];
        let mut names = BTreeMap::new();
        let mut at = 0usize;
        while at + 5 < buf.len() {
            // Entries are signature, 0x09, name. The section is padded to
            // 16 with 0xFF, and a signature can start with 0xFF, so the
            // separator is the reliable end marker.
            if buf[at + 4] != 0x09 {
                break;
            }
            let name = c_string(buf, at + 5).ok_or(Error::BadString(at as u32))?;
            names.insert(at as u32, name.to_string());
            names.insert(at as u32 + 5, name.to_string());
            at += 5 + name.len() + 1;
        }
        Ok(names)
    }

    pub fn pointer_size(&self) -> usize {
        self.header.pointer_size as usize
    }

    /// The object the header names as the file's contents, normally the
    /// `hkRootLevelContainer` at data offset 0.
    pub fn root(&self) -> Result<Obj<'_>> {
        if self.header.contents_section_index as u32 != self.data_section_index {
            return Err(Error::MissingSection("contents in __data__"));
        }
        let off = self.header.contents_section_offset as u32;
        if off as usize >= self.data.len() {
            return Err(Error::BadFixup {
                kind: "contents",
                src: 0,
                dst: off,
            });
        }
        Ok(Obj {
            pf: self,
            base: off,
        })
    }

    /// A view over the object at a data offset.
    pub fn object(&self, offset: u32) -> Obj<'_> {
        Obj {
            pf: self,
            base: offset,
        }
    }

    /// Every object of one class, by data offset.
    pub fn objects_of(&self, class: &str) -> Vec<Obj<'_>> {
        self.objects
            .iter()
            .filter(|(_, c)| c.as_str() == class)
            .map(|(&off, _)| self.object(off))
            .collect()
    }

    /// Refuse a file whose pointer size does not match the layouts.
    pub fn require_64bit(&self) -> Result<()> {
        if self.header.pointer_size == 8 {
            Ok(())
        } else {
            Err(Error::PointerSize(self.header.pointer_size))
        }
    }

    fn string_at(&self, dst: u32) -> Result<String> {
        c_string(&self.data, dst as usize)
            .map(str::to_string)
            .ok_or(Error::BadString(dst))
    }
}

/// A bounds-checked view over one object in the data section. Field
/// accessors take the field's offset within the object.
#[derive(Clone, Copy)]
pub struct Obj<'a> {
    pf: &'a Packfile,
    pub base: u32,
}

impl std::fmt::Debug for Obj<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Obj({} @ {})", self.class().unwrap_or("?"), self.base)
    }
}

impl<'a> Obj<'a> {
    /// Select a byte offset or element stride from Havok's x86 and x64 layouts.
    pub fn layout_offset(&self, x86: u32, x64: u32) -> u32 {
        if self.pf.pointer_size() == 4 {
            x86
        } else {
            x64
        }
    }

    /// Read an `hkUlong`, whose width follows the file's pointer size.
    pub fn ulong(&self, off: u32) -> Result<u64> {
        if self.pf.pointer_size() == 4 {
            self.u32(off).map(u64::from)
        } else {
            self.u64(off)
        }
    }

    pub fn packfile(&self) -> &'a Packfile {
        self.pf
    }

    /// The class this object was written as, when a virtual fixup names it.
    /// Sub-objects embedded by value (array elements, nested structs) have
    /// none.
    pub fn class(&self) -> Option<&'a str> {
        self.pf.objects.get(&self.base).map(String::as_str)
    }

    pub fn expect_class(&self, expected: &'static str) -> Result<()> {
        match self.class() {
            Some(c) if c == expected => Ok(()),
            other => Err(Error::WrongClass {
                expected,
                found: other.unwrap_or("<unnamed>").to_string(),
            }),
        }
    }

    /// A view over an embedded struct at a field offset.
    pub fn at(&self, off: u32) -> Obj<'a> {
        Obj {
            pf: self.pf,
            base: self.base.wrapping_add(off),
        }
    }

    pub fn bytes(&self, off: u32, n: usize) -> Result<&'a [u8]> {
        let at = self.base as usize + off as usize;
        need("field", &self.pf.data, at, n)?;
        Ok(&self.pf.data[at..at + n])
    }

    pub fn u8(&self, off: u32) -> Result<u8> {
        Ok(self.bytes(off, 1)?[0])
    }
    pub fn i8(&self, off: u32) -> Result<i8> {
        Ok(self.bytes(off, 1)?[0] as i8)
    }
    pub fn bool(&self, off: u32) -> Result<bool> {
        Ok(self.u8(off)? != 0)
    }
    pub fn u16(&self, off: u32) -> Result<u16> {
        let b = self.bytes(off, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn i16(&self, off: u32) -> Result<i16> {
        Ok(self.u16(off)? as i16)
    }
    pub fn u32(&self, off: u32) -> Result<u32> {
        Ok(le_u32(self.bytes(off, 4)?))
    }
    pub fn i32(&self, off: u32) -> Result<i32> {
        Ok(le_i32(self.bytes(off, 4)?))
    }
    pub fn u64(&self, off: u32) -> Result<u64> {
        let b = self.bytes(off, 8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    pub fn f32(&self, off: u32) -> Result<f32> {
        Ok(f32::from_bits(self.u32(off)?))
    }
    pub fn vec4(&self, off: u32) -> Result<[f32; 4]> {
        Ok([
            self.f32(off)?,
            self.f32(off + 4)?,
            self.f32(off + 8)?,
            self.f32(off + 12)?,
        ])
    }
    pub fn qs_transform(&self, off: u32) -> Result<QsTransform> {
        Ok(QsTransform {
            translation: self.vec4(off)?,
            rotation: self.vec4(off + 16)?,
            scale: self.vec4(off + 32)?,
        })
    }

    fn field_pos(&self, off: u32) -> u32 {
        self.base.wrapping_add(off)
    }

    /// The object a pointer field points at, or `None` for a null pointer.
    pub fn ptr(&self, off: u32) -> Result<Option<Obj<'a>>> {
        match self.pf.global_fixups.get(&self.field_pos(off)) {
            None => Ok(None),
            Some(&(section, dst)) => {
                if section != self.pf.data_section_index {
                    return Err(Error::Unsupported(format!(
                        "pointer into section {section}"
                    )));
                }
                Ok(Some(Obj {
                    pf: self.pf,
                    base: dst,
                }))
            }
        }
    }

    pub fn ptr_required(&self, off: u32, what: &'static str) -> Result<Obj<'a>> {
        self.ptr(off)?.ok_or(Error::NullPointer(what))
    }

    /// An `hkStringPtr` or `char*` field. Null pointers read as `None`.
    pub fn string(&self, off: u32) -> Result<Option<String>> {
        match self.pf.local_fixups.get(&self.field_pos(off)) {
            None => Ok(None),
            Some(&dst) => self.pf.string_at(dst).map(Some),
        }
    }

    pub fn string_or_empty(&self, off: u32) -> Result<String> {
        Ok(self.string(off)?.unwrap_or_default())
    }

    /// The contents position and element count of an `hkArray` field.
    /// An array with no local fixup is empty.
    pub fn array(&self, off: u32) -> Result<Option<(u32, usize)>> {
        let count_off = off + self.pf.pointer_size() as u32;
        let count = self.i32(count_off)?;
        if count < 0 {
            return Err(Error::BadFixup {
                kind: "array count",
                src: self.field_pos(off),
                dst: count as u32,
            });
        }
        match self.pf.local_fixups.get(&self.field_pos(off)) {
            None => Ok(None),
            Some(&dst) => Ok(Some((dst, count as usize))),
        }
    }

    /// Element views over an array of structs of `stride` bytes.
    pub fn array_structs(&self, off: u32, stride: u32) -> Result<Vec<Obj<'a>>> {
        let Some((dst, count)) = self.array(off)? else {
            return Ok(Vec::new());
        };
        let total = (count as u64) * (stride as u64);
        need(
            "array contents",
            &self.pf.data,
            dst as usize,
            usize::try_from(total).map_err(|_| Error::BadFixup {
                kind: "array size",
                src: self.field_pos(off),
                dst,
            })?,
        )?;
        Ok((0..count as u32)
            .map(|i| Obj {
                pf: self.pf,
                base: dst + i * stride,
            })
            .collect())
    }

    /// An `hkArray<T*>`: each element is a pointer resolved by a global fixup.
    pub fn array_ptrs(&self, off: u32) -> Result<Vec<Option<Obj<'a>>>> {
        let ps = self.pf.pointer_size() as u32;
        self.array_structs(off, ps)?
            .into_iter()
            .map(|slot| slot.ptr(0))
            .collect()
    }

    pub fn array_strings(&self, off: u32) -> Result<Vec<String>> {
        let ps = self.pf.pointer_size() as u32;
        self.array_structs(off, ps)?
            .into_iter()
            .map(|slot| slot.string_or_empty(0))
            .collect()
    }

    pub fn array_u8(&self, off: u32) -> Result<Vec<u8>> {
        let Some((dst, count)) = self.array(off)? else {
            return Ok(Vec::new());
        };
        need("array contents", &self.pf.data, dst as usize, count)?;
        Ok(self.pf.data[dst as usize..dst as usize + count].to_vec())
    }

    pub fn array_i16(&self, off: u32) -> Result<Vec<i16>> {
        self.array_structs(off, 2)?
            .into_iter()
            .map(|e| e.i16(0))
            .collect()
    }

    pub fn array_i32(&self, off: u32) -> Result<Vec<i32>> {
        self.array_structs(off, 4)?
            .into_iter()
            .map(|e| e.i32(0))
            .collect()
    }

    pub fn array_u32(&self, off: u32) -> Result<Vec<u32>> {
        self.array_structs(off, 4)?
            .into_iter()
            .map(|e| e.u32(0))
            .collect()
    }

    pub fn array_f32(&self, off: u32) -> Result<Vec<f32>> {
        self.array_structs(off, 4)?
            .into_iter()
            .map(|e| e.f32(0))
            .collect()
    }

    pub fn array_vec4(&self, off: u32) -> Result<Vec<[f32; 4]>> {
        self.array_structs(off, 16)?
            .into_iter()
            .map(|e| e.vec4(0))
            .collect()
    }

    pub fn array_qs_transform(&self, off: u32) -> Result<Vec<QsTransform>> {
        self.array_structs(off, 48)?
            .into_iter()
            .map(|e| e.qs_transform(0))
            .collect()
    }
}

/// The `hkRootLevelContainer`: named variants, each a class name and an
/// object pointer.
#[derive(Debug, Clone)]
pub struct NamedVariant<'a> {
    pub name: String,
    pub class_name: String,
    pub object: Option<Obj<'a>>,
}

pub fn root_variants(pf: &Packfile) -> Result<Vec<NamedVariant<'_>>> {
    let root = pf.root()?;
    root.expect_class("hkRootLevelContainer")?;
    let stride = 3 * pf.pointer_size() as u32;
    root.array_structs(0, stride)?
        .into_iter()
        .map(|v| {
            Ok(NamedVariant {
                name: v.string_or_empty(0)?,
                class_name: v.string_or_empty(pf.pointer_size() as u32)?,
                object: v.ptr(2 * pf.pointer_size() as u32)?,
            })
        })
        .collect()
}

/// The first root variant of a class, by the class the variant declares.
pub fn root_variant_of<'a>(pf: &'a Packfile, class: &'static str) -> Result<Obj<'a>> {
    root_variants(pf)?
        .into_iter()
        .find(|v| v.class_name == class)
        .and_then(|v| v.object)
        .ok_or(Error::NullPointer(class))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::PackfileBuilder;

    #[test]
    fn bad_magic_is_not_a_packfile() {
        assert_eq!(
            Packfile::parse(&[0u8; 64]).unwrap_err(),
            Error::NotAPackfile
        );
    }

    #[test]
    fn a_short_header_is_truncated_not_a_panic() {
        let e = Packfile::parse(&[0x57, 0xE0, 0xE0, 0x57]).unwrap_err();
        assert!(matches!(e, Error::Truncated { what: "header", .. }));
    }

    #[test]
    fn a_file_cut_inside_the_data_section_is_reported() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        b.set_i32(root, 8, 0);
        b.set_i32(root, 12, i32::MIN);
        let bytes = b.finish();
        let cut = &bytes[..bytes.len() - 40];
        let e = Packfile::parse(cut).unwrap_err();
        assert!(matches!(e, Error::Truncated { .. }), "{e}");
    }

    #[test]
    fn a_fixup_outside_the_section_is_rejected() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        b.set_i32(root, 12, i32::MIN);
        let mut bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        // Rewrite the single virtual fixup's src to point past the data.
        let ds = pf.sections.iter().find(|s| s.tag == "__data__").unwrap();
        let at = ds.absolute_data_start as usize + ds.virtual_fixups_offset as usize;
        bytes[at..at + 4].copy_from_slice(&0x7FFF_FFF0u32.to_le_bytes());
        let e = Packfile::parse(&bytes).unwrap_err();
        assert!(
            matches!(
                e,
                Error::BadFixup {
                    kind: "virtual",
                    ..
                }
            ),
            "{e}"
        );
    }

    #[test]
    fn field_reads_past_the_object_are_errors() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        b.set_i32(root, 12, i32::MIN);
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        let root = pf.root().unwrap();
        assert_eq!(root.class(), Some("hkRootLevelContainer"));
        assert!(root.u32(0).is_ok());
        assert!(matches!(root.u32(4096), Err(Error::Truncated { .. })));
        assert!(root.ptr(0).unwrap().is_none());
        assert!(root.string(0).unwrap().is_none());
    }

    #[test]
    fn strings_and_pointer_arrays_resolve_through_fixups() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let child = b.object("hkaSkeleton", 120);
        b.string(child, 16, "Bip01");
        let slot = b.object("hkRootLevelContainerNamedVariant", 24);
        b.string(slot, 0, "Merged Animation Container");
        b.string(slot, 8, "hkaAnimationContainer");
        b.ptr(slot, 16, child);
        b.array_ptrs(root, 0, &[child, child]);
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        let root = pf.root().unwrap();
        let ptrs = root.array_ptrs(0).unwrap();
        assert_eq!(ptrs.len(), 2);
        let c = ptrs[0].unwrap();
        assert_eq!(c.class(), Some("hkaSkeleton"));
        assert_eq!(c.string(16).unwrap().as_deref(), Some("Bip01"));
        let s = pf.object(slot);
        assert_eq!(
            s.string(8).unwrap().as_deref(),
            Some("hkaAnimationContainer")
        );
        assert_eq!(s.ptr(16).unwrap().unwrap().base, child);
        assert_eq!(pf.objects_of("hkaSkeleton").len(), 1);
    }
}
