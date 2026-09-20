// SPDX-License-Identifier: GPL-3.0-or-later

//! Editable Havok 2010.2 memory images, with opaque payloads and explicit fixups.
//! Follows serde-hkx `bytes/ser` and HavokLib `format_old.cpp`. Payload offsets
//! remain stable while section placement and fixup tables are rebuilt.

use std::collections::BTreeSet;

use crate::packfile::{read_headers, Header, SECTION_HEADER_SIZE};
use crate::{Error, Result};

/// A section-relative address, independent of the section's file placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Address {
    pub section: u32,
    pub offset: u32,
}

/// A pointer field and its target in the same section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalFixup {
    pub source: u32,
    pub target: u32,
}

/// A pointer field or object and its target in another section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixup {
    pub source: u32,
    pub target: Address,
}

/// A memory image including objects the engine does not model.
/// Edits must preserve existing payload offsets or update all affected fixups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Original tag bytes, including the separator byte.
    pub tag: [u8; 20],
    /// Objects, array storage, strings and their original padding.
    pub data: Vec<u8>,
    pub local: Vec<LocalFixup>,
    pub global: Vec<Fixup>,
    /// Object starts and class name addresses.
    pub virtual_: Vec<Fixup>,
}

/// A validated, editable packfile, without an original-file passthrough buffer.
/// The header extension is retained. Section headers and relocations are rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackfileDocument {
    header: Header,
    prefix: Vec<u8>,
    pub sections: Vec<Section>,
}

fn bad(kind: &'static str, src: u32, dst: u32) -> Error {
    Error::BadFixup { kind, src, dst }
}

fn word(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn size(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| Error::Unsupported("packfile exceeds 32-bit offsets".into()))
}

fn records<const N: usize>(bytes: &[u8]) -> Result<Vec<[u32; N]>> {
    let mut records = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        if rest.iter().all(|&b| b == 0xff) {
            break;
        }
        if rest.len() < N * 4 || word(rest) == u32::MAX {
            return Err(bad("incomplete relocation table", size(at)?, 0));
        }
        records.push(std::array::from_fn(|i| word(&rest[i * 4..])));
        at += N * 4;
    }
    Ok(records)
}

impl PackfileDocument {
    /// Decode every section and relocation, retaining unmodeled object bytes.
    ///
    /// # Errors
    /// Rejects malformed ranges, relocations and class targets. Only version 8
    /// little-endian packfiles without import/export tables are supported.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let (header, headers, table_start) = read_headers(bytes)?;
        if header.file_version != 8 || header.version_string != "hk_2010.2.0-r1" {
            return Err(Error::Unsupported(
                "writer requires Havok 2010.2 version 8".into(),
            ));
        }
        let table_end = table_start + headers.len() * SECTION_HEADER_SIZE;
        let mut ranges = Vec::new();
        let mut sections = Vec::new();
        for (i, h) in headers.iter().enumerate() {
            let offsets = [
                h.local_fixups_offset,
                h.global_fixups_offset,
                h.virtual_fixups_offset,
                h.exports_offset,
                h.imports_offset,
                h.end_offset,
            ];
            let start = h.absolute_data_start as usize;
            let end = start
                .checked_add(h.end_offset as usize)
                .ok_or_else(|| bad("section overflow", i as u32, h.end_offset))?;
            if start < table_end || end > bytes.len() || offsets.windows(2).any(|p| p[0] > p[1]) {
                return Err(bad("section range", i as u32, h.end_offset));
            }
            if h.exports_offset != h.end_offset || h.imports_offset != h.end_offset {
                return Err(Error::Unsupported("packfile import/export tables".into()));
            }
            if start != end {
                ranges.push((start, end));
            }
            let data = &bytes[start..end];
            let local = records::<2>(&data[offsets[0] as usize..offsets[1] as usize])?
                .into_iter()
                .map(|[source, target]| LocalFixup { source, target })
                .collect();
            let fixups = |lo: u32, hi: u32| -> Result<Vec<Fixup>> {
                Ok(records::<3>(&data[lo as usize..hi as usize])?
                    .into_iter()
                    .map(|[source, section, offset]| Fixup {
                        source,
                        target: Address { section, offset },
                    })
                    .collect())
            };
            let mut tag = [0; 20];
            let at = table_start + i * SECTION_HEADER_SIZE;
            tag.copy_from_slice(&bytes[at..at + 20]);
            sections.push(Section {
                tag,
                data: data[..offsets[0] as usize].to_vec(),
                local,
                global: fixups(offsets[1], offsets[2])?,
                virtual_: fixups(offsets[2], offsets[3])?,
            });
        }
        ranges.sort_unstable();
        if ranges.windows(2).any(|p| p[0].1 > p[1].0) {
            return Err(bad("overlapping sections", 0, 0));
        }
        let doc = Self {
            header,
            prefix: bytes[..table_start].to_vec(),
            sections,
        };
        doc.validate()?;
        Ok(doc)
    }

    /// Header metadata from the source file.
    pub fn header(&self) -> &Header {
        &self.header
    }

    fn target_exists(&self, target: Address) -> bool {
        self.sections
            .get(target.section as usize)
            .is_some_and(|s| (target.offset as usize) < s.data.len())
    }

    fn class_addresses(&self) -> Result<BTreeSet<Address>> {
        let mut names = BTreeSet::new();
        for (i, s) in self.sections.iter().enumerate() {
            if !s.tag.starts_with(b"__classnames__\0") {
                continue;
            }
            let mut at = 0;
            while at < s.data.len() {
                let rest = &s.data[at..];
                if rest.iter().all(|&b| b == 0xff) {
                    break;
                }
                if rest.len() < 6 || rest[4] != 9 {
                    return Err(Error::BadString(size(at)?));
                }
                let name = &rest[5..];
                let len = name
                    .iter()
                    .position(|&b| b == 0)
                    .ok_or(Error::BadString(size(at)?))?;
                if len == 0 || std::str::from_utf8(&name[..len]).is_err() {
                    return Err(Error::BadString(size(at)?));
                }
                names.insert(Address {
                    section: i as u32,
                    offset: size(at)?,
                });
                names.insert(Address {
                    section: i as u32,
                    offset: size(at + 5)?,
                });
                at += 6 + len;
            }
        }
        Ok(names)
    }

    /// Validate edits before writing any output.
    ///
    /// # Errors
    /// Rejects invalid or overlapping pointer slots, duplicate object records,
    /// missing target sections, out-of-range targets and invalid root metadata.
    pub fn validate(&self) -> Result<()> {
        let names = self.class_addresses()?;
        let width = u32::from(self.header.pointer_size);
        for (i, s) in self.sections.iter().enumerate() {
            let len = size(s.data.len())?;
            let mut sources = BTreeSet::new();
            for f in &s.local {
                if f.source.checked_add(width).is_none_or(|end| end > len)
                    || f.target >= len
                    || !sources.insert(f.source)
                {
                    return Err(bad("local", f.source, f.target));
                }
            }
            for f in &s.global {
                if f.source.checked_add(width).is_none_or(|end| end > len)
                    || !self.target_exists(f.target)
                    || !sources.insert(f.source)
                {
                    return Err(bad("global", f.source, f.target.offset));
                }
            }
            let mut previous = None;
            for source in sources {
                if previous.is_some_and(|p| source < p + width) {
                    return Err(bad("overlapping pointer slots", source, i as u32));
                }
                previous = Some(source);
            }
            let mut objects = BTreeSet::new();
            for f in &s.virtual_ {
                if f.source >= len || !names.contains(&f.target) || !objects.insert(f.source) {
                    return Err(bad("virtual", f.source, f.target.offset));
                }
            }
        }
        let root = Address {
            section: self.header.contents_section_index as u32,
            offset: self.header.contents_section_offset as u32,
        };
        let class = Address {
            section: self.header.contents_class_name_section_index as u32,
            offset: self.header.contents_class_name_section_offset as u32,
        };
        if !self.target_exists(root) || !names.contains(&class) {
            return Err(bad("contents", root.offset, class.offset));
        }
        if !self.sections[root.section as usize]
            .virtual_
            .iter()
            .any(|f| f.source == root.offset && f.target == class)
        {
            return Err(bad("contents class", root.offset, class.offset));
        }
        Ok(())
    }

    /// Rebuild section placement and fixup tables from the edited document.
    /// No object layout knowledge is needed to preserve opaque payloads.
    ///
    /// # Errors
    /// Returns validation errors or an error if a file offset exceeds 32 bits.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        if self.sections.len() > 64 {
            return Err(Error::Unsupported("more than 64 sections".into()));
        }
        let table_start = self.prefix.len();
        let mut out = self.prefix.clone();
        out[20..24].copy_from_slice(&size(self.sections.len())?.to_le_bytes());
        out.resize(table_start + self.sections.len() * SECTION_HEADER_SIZE, 0);
        for (i, s) in self.sections.iter().enumerate() {
            out.resize(out.len().div_ceil(16) * 16, 0);
            let start = size(out.len())?;
            let mut body = s.data.clone();
            let local = size(body.len())?;
            for f in &s.local {
                push_words(&mut body, &[f.source, f.target]);
            }
            body.resize(body.len().div_ceil(16) * 16, 0xff);
            let global = size(body.len())?;
            for f in &s.global {
                push_words(&mut body, &[f.source, f.target.section, f.target.offset]);
            }
            body.resize(body.len().div_ceil(16) * 16, 0xff);
            let virtual_ = size(body.len())?;
            for f in &s.virtual_ {
                push_words(&mut body, &[f.source, f.target.section, f.target.offset]);
            }
            body.resize(body.len().div_ceil(16) * 16, 0xff);
            let end = size(body.len())?;
            let at = table_start + i * SECTION_HEADER_SIZE;
            out[at..at + 20].copy_from_slice(&s.tag);
            for (j, v) in [start, local, global, virtual_, end, end, end]
                .iter()
                .enumerate()
            {
                out[at + 20 + j * 4..at + 24 + j * 4].copy_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&body);
            size(out.len())?;
        }
        Ok(out)
    }
}

fn push_words(out: &mut Vec<u8>, values: &[u32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}
