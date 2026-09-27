// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin file: a TES4 header record followed by top-level groups. Every
//! record is kept as its subrecord list so unknown record types round-trip
//! byte for byte (compression aside), and the group tree is kept so the
//! placement of cells, references, land and navmeshes is known.
//!
//! Layout, Skyrim SE (form version 40 and later, 24-byte headers):
//!
//! ```text
//! record  tag[4] data_size:u32 flags:u32 form_id:u32 timestamp:u16 vcs:u16 form_version:u16 vcs2:u16 data[data_size]
//! group   "GRUP" group_size:u32 label[4] group_type:i32 timestamp:u16 vcs:u16 version:u16 unknown:u16 children[group_size - 24]
//! sub     tag[4] size:u16 data[size]
//! ```
//!
//! `group_size` counts the header. A subrecord larger than 65535 bytes is
//! preceded by an `XXXX` subrecord whose four-byte payload is the real size
//! and whose own size field is zero. A record with flag 0x00040000 stores a
//! u32 inflated size followed by a zlib stream.

use sha2::{Digest, Sha256};
use std::io::Read;

use crate::cursor::Cursor;
use crate::error::{Error, Result};
use crate::tag::{self, Tag};

pub const HEADER_SIZE: usize = 24;
pub const SUBRECORD_HEADER_SIZE: usize = 6;

/// Upper bound on the up-front allocation for a compressed record's inflate
/// buffer. The 32-bit "expected size" prefix is attacker controlled and read
/// before a single byte of the zlib stream is checked, so a crafted file can
/// claim close to 4 GiB there. No vanilla or modded record comes near this
/// bound, so capping the preallocation only costs a realloc on the (never
/// seen) legitimate record that exceeds it, while a hostile claim still gets
/// caught by the `buf.len() != expected as usize` check right below.
const MAX_INFLATE_PREALLOC: usize = 64 * 1024 * 1024;

/// Record header flags shared by every record type. Type-specific bits are
/// documented on the typed views.
pub mod flags {
    pub const MASTER: u32 = 0x0000_0001;
    pub const DELETED: u32 = 0x0000_0020;
    pub const LOCALIZED: u32 = 0x0000_0080;
    pub const LIGHT_MASTER: u32 = 0x0000_0200;
    pub const PERSISTENT: u32 = 0x0000_0400;
    pub const INITIALLY_DISABLED: u32 = 0x0000_0800;
    pub const IGNORED: u32 = 0x0000_1000;
    pub const VISIBLE_WHEN_DISTANT: u32 = 0x0000_8000;
    pub const COMPRESSED: u32 = 0x0004_0000;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subrecord {
    pub tag: Tag,
    pub data: Vec<u8>,
}

impl Subrecord {
    pub fn new(tag: impl Into<Tag>, data: impl Into<Vec<u8>>) -> Self {
        Subrecord {
            tag: tag.into(),
            data: data.into(),
        }
    }
}

/// Where a record sits in the group tree, as raw (file-local) FormIDs of the
/// enclosing worldspace and cell, filled in by the parser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placement {
    pub world: Option<u32>,
    pub cell: Option<u32>,
    /// The innermost group's kind, for telling persistent from temporary
    /// cell children.
    pub group: Option<GroupKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub tag: Tag,
    pub flags: u32,
    /// The FormID as written in the file. Resolve it through a `LoadOrder`.
    pub form_id: u32,
    pub timestamp: u16,
    pub vcs: u16,
    pub form_version: u16,
    pub vcs2: u16,
    pub subrecords: Vec<Subrecord>,
    pub placement: Placement,
    /// Absolute offset of the record header in the file it was read from.
    pub offset: usize,
}

impl Record {
    pub fn new(tag: impl Into<Tag>, form_id: u32) -> Self {
        Record {
            tag: tag.into(),
            flags: 0,
            form_id,
            timestamp: 0,
            vcs: 0,
            form_version: 44,
            vcs2: 0,
            subrecords: Vec::new(),
            placement: Placement::default(),
            offset: 0,
        }
    }

    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    pub fn sub(mut self, tag: impl Into<Tag>, data: impl Into<Vec<u8>>) -> Self {
        self.subrecords.push(Subrecord::new(tag, data));
        self
    }

    /// A NUL-terminated string subrecord.
    pub fn zsub(self, tag: impl Into<Tag>, text: &str) -> Self {
        let mut data = text.as_bytes().to_vec();
        data.push(0);
        self.sub(tag, data)
    }

    pub fn find(&self, tag: Tag) -> Option<&[u8]> {
        self.subrecords
            .iter()
            .find(|s| s.tag == tag)
            .map(|s| s.data.as_slice())
    }

    pub fn all(&self, tag: Tag) -> impl Iterator<Item = &[u8]> {
        self.subrecords
            .iter()
            .filter(move |s| s.tag == tag)
            .map(|s| s.data.as_slice())
    }

    pub fn required(&self, tag: Tag) -> Result<&[u8]> {
        self.find(tag).ok_or(Error::MissingSubrecord {
            record: self.tag,
            tag,
        })
    }

    pub fn editor_id(&self) -> Option<String> {
        self.find(tag::EDID).map(crate::cursor::zstring)
    }

    pub fn is_deleted(&self) -> bool {
        self.flags & flags::DELETED != 0
    }

    pub fn is_compressed(&self) -> bool {
        self.flags & flags::COMPRESSED != 0
    }

    /// Serialize uncompressed. The compressed flag is cleared in the output
    /// so the bytes describe themselves.
    pub fn write(&self, out: &mut Vec<u8>) {
        let mut body = Vec::new();
        for s in &self.subrecords {
            if s.data.len() > u16::MAX as usize {
                body.extend_from_slice(tag::XXXX.as_bytes());
                body.extend_from_slice(&4u16.to_le_bytes());
                body.extend_from_slice(&(s.data.len() as u32).to_le_bytes());
                body.extend_from_slice(s.tag.as_bytes());
                body.extend_from_slice(&0u16.to_le_bytes());
            } else {
                body.extend_from_slice(s.tag.as_bytes());
                body.extend_from_slice(&(s.data.len() as u16).to_le_bytes());
            }
            body.extend_from_slice(&s.data);
        }
        out.extend_from_slice(self.tag.as_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.flags & !flags::COMPRESSED).to_le_bytes());
        out.extend_from_slice(&self.form_id.to_le_bytes());
        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out.extend_from_slice(&self.vcs.to_le_bytes());
        out.extend_from_slice(&self.form_version.to_le_bytes());
        out.extend_from_slice(&self.vcs2.to_le_bytes());
        out.extend_from_slice(&body);
    }
}

/// Group label and type together, since the type decides how the four label
/// bytes are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupKind {
    /// Type 0: all records of one signature.
    Top(Tag),
    /// Type 1: children of a WRLD, labelled with its FormID.
    WorldChildren(u32),
    /// Type 2: interior cell block, labelled with the block number.
    InteriorBlock(i32),
    /// Type 3: interior cell sub-block.
    InteriorSubBlock(i32),
    /// Type 4: exterior cell block, labelled with a grid position. The file
    /// stores Y before X.
    ExteriorBlock { y: i16, x: i16 },
    /// Type 5: exterior cell sub-block.
    ExteriorSubBlock { y: i16, x: i16 },
    /// Type 6: children of a CELL, labelled with its FormID.
    CellChildren(u32),
    /// Type 7: children of a DIAL topic.
    TopicChildren(u32),
    /// Type 8: persistent references of a cell.
    CellPersistent(u32),
    /// Type 9: temporary references, LAND and NAVM of a cell.
    CellTemporary(u32),
    /// Type 10: visible-when-distant children (Oblivion only, kept for
    /// completeness).
    CellVisibleDistant(u32),
}

impl GroupKind {
    fn from_raw(at: usize, label: [u8; 4], kind: i32) -> Result<Self> {
        let id = u32::from_le_bytes(label);
        let y = i16::from_le_bytes([label[0], label[1]]);
        let x = i16::from_le_bytes([label[2], label[3]]);
        Ok(match kind {
            0 => GroupKind::Top(Tag(label)),
            1 => GroupKind::WorldChildren(id),
            2 => GroupKind::InteriorBlock(id as i32),
            3 => GroupKind::InteriorSubBlock(id as i32),
            4 => GroupKind::ExteriorBlock { y, x },
            5 => GroupKind::ExteriorSubBlock { y, x },
            6 => GroupKind::CellChildren(id),
            7 => GroupKind::TopicChildren(id),
            8 => GroupKind::CellPersistent(id),
            9 => GroupKind::CellTemporary(id),
            10 => GroupKind::CellVisibleDistant(id),
            other => return Err(Error::UnknownGroupType { at, kind: other }),
        })
    }

    pub fn raw(&self) -> ([u8; 4], i32) {
        let grid = |y: i16, x: i16| {
            let yb = y.to_le_bytes();
            let xb = x.to_le_bytes();
            [yb[0], yb[1], xb[0], xb[1]]
        };
        match *self {
            GroupKind::Top(t) => (t.0, 0),
            GroupKind::WorldChildren(id) => (id.to_le_bytes(), 1),
            GroupKind::InteriorBlock(n) => (n.to_le_bytes(), 2),
            GroupKind::InteriorSubBlock(n) => (n.to_le_bytes(), 3),
            GroupKind::ExteriorBlock { y, x } => (grid(y, x), 4),
            GroupKind::ExteriorSubBlock { y, x } => (grid(y, x), 5),
            GroupKind::CellChildren(id) => (id.to_le_bytes(), 6),
            GroupKind::TopicChildren(id) => (id.to_le_bytes(), 7),
            GroupKind::CellPersistent(id) => (id.to_le_bytes(), 8),
            GroupKind::CellTemporary(id) => (id.to_le_bytes(), 9),
            GroupKind::CellVisibleDistant(id) => (id.to_le_bytes(), 10),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Index into `Plugin::records`.
    Record(usize),
    Group(Group),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub kind: GroupKind,
    pub timestamp: u16,
    pub vcs: u16,
    pub version: u16,
    pub unknown: u16,
    pub entries: Vec<Entry>,
}

impl Group {
    pub fn new(kind: GroupKind) -> Self {
        Group {
            kind,
            timestamp: 0,
            vcs: 0,
            version: 0,
            unknown: 0,
            entries: Vec::new(),
        }
    }

    fn write(&self, records: &[Record], out: &mut Vec<u8>) {
        let mut body = Vec::new();
        for e in &self.entries {
            match e {
                Entry::Record(i) => records[*i].write(&mut body),
                Entry::Group(g) => g.write(records, &mut body),
            }
        }
        let (label, kind) = self.kind.raw();
        out.extend_from_slice(tag::GRUP.as_bytes());
        out.extend_from_slice(&((body.len() + HEADER_SIZE) as u32).to_le_bytes());
        out.extend_from_slice(&label);
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out.extend_from_slice(&self.vcs.to_le_bytes());
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.unknown.to_le_bytes());
        out.extend_from_slice(&body);
    }

    /// Every record index in this group and its descendants, in file order.
    pub fn record_indices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<usize>) {
        for e in &self.entries {
            match e {
                Entry::Record(i) => out.push(*i),
                Entry::Group(g) => g.collect(out),
            }
        }
    }
}

/// The TES4 file header, decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct FileHeader {
    pub flags: u32,
    pub version: f32,
    pub record_count: u32,
    pub next_object_id: u32,
    pub author: String,
    pub description: String,
    pub masters: Vec<String>,
    /// ONAM: forms this plugin overrides that the game must treat as such
    /// even when the override is identical (references, LAND, NAVM).
    pub overridden_forms: Vec<u32>,
    pub interior_cell_count: Option<u32>,
}

impl FileHeader {
    pub fn is_master(&self) -> bool {
        self.flags & flags::MASTER != 0
    }

    pub fn is_light(&self) -> bool {
        self.flags & flags::LIGHT_MASTER != 0
    }

    pub fn is_localized(&self) -> bool {
        self.flags & flags::LOCALIZED != 0
    }

    pub fn from_record(rec: &Record) -> Result<Self> {
        let mut f = crate::cursor::Fields::new(
            rec.tag,
            Tag::new(b"HEDR"),
            rec.required(Tag::new(b"HEDR"))?,
        );
        f.exact(12)?;
        let version = f.f32()?;
        let record_count = f.u32()?;
        let next_object_id = f.u32()?;
        let mut overridden_forms = Vec::new();
        if let Some(onam) = rec.find(Tag::new(b"ONAM")) {
            overridden_forms.extend(
                onam.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| u32::from_le_bytes(*c)),
            );
        }
        let interior_cell_count = rec
            .find(Tag::new(b"INCC"))
            .and_then(|d| (d.len() == 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]])));
        Ok(FileHeader {
            flags: rec.flags,
            version,
            record_count,
            next_object_id,
            author: rec
                .find(Tag::new(b"CNAM"))
                .map(crate::cursor::zstring)
                .unwrap_or_default(),
            description: rec
                .find(Tag::new(b"SNAM"))
                .map(crate::cursor::zstring)
                .unwrap_or_default(),
            masters: rec
                .all(Tag::new(b"MAST"))
                .map(crate::cursor::zstring)
                .collect(),
            overridden_forms,
            interior_cell_count,
        })
    }

    /// Build the TES4 record for this header.
    pub fn to_record(&self) -> Record {
        let mut hedr = Vec::with_capacity(12);
        hedr.extend_from_slice(&self.version.to_le_bytes());
        hedr.extend_from_slice(&self.record_count.to_le_bytes());
        hedr.extend_from_slice(&self.next_object_id.to_le_bytes());
        let mut rec = Record::new(tag::TES4, 0)
            .with_flags(self.flags)
            .sub(Tag::new(b"HEDR"), hedr)
            .zsub(Tag::new(b"CNAM"), &self.author);
        if !self.description.is_empty() {
            rec = rec.zsub(Tag::new(b"SNAM"), &self.description);
        }
        for m in &self.masters {
            rec = rec
                .zsub(Tag::new(b"MAST"), m)
                .sub(Tag::new(b"DATA"), [0u8; 8]);
        }
        if !self.overridden_forms.is_empty() {
            let mut onam = Vec::new();
            for id in &self.overridden_forms {
                onam.extend_from_slice(&id.to_le_bytes());
            }
            rec = rec.sub(Tag::new(b"ONAM"), onam);
        }
        rec = rec.sub(Tag::new(b"INTV"), 0u32.to_le_bytes());
        if let Some(n) = self.interior_cell_count {
            rec = rec.sub(Tag::new(b"INCC"), n.to_le_bytes());
        }
        rec
    }
}

impl Default for FileHeader {
    fn default() -> Self {
        FileHeader {
            flags: 0,
            version: 1.71,
            record_count: 0,
            next_object_id: 0x800,
            author: String::new(),
            description: String::new(),
            masters: Vec::new(),
            overridden_forms: Vec::new(),
            interior_cell_count: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plugin {
    pub header: FileHeader,
    /// The TES4 record as read, for round-tripping subrecords the decoded
    /// header does not model.
    pub header_record: Record,
    pub records: Vec<Record>,
    pub groups: Vec<Group>,
}

impl Plugin {
    /// An empty plugin with the given header, for building files.
    pub fn new(header: FileHeader) -> Self {
        Plugin {
            header_record: header.to_record(),
            header,
            records: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Plugin> {
        let _span = soe_trace::Span::with_payload("esm.plugin_parse", "esm", bytes.len() as u64);
        let mut cur = Cursor::new(bytes, 0);
        let first = cur.peek_tag()?;
        if first != tag::TES4 {
            return Err(Error::NoHeader {
                at: 0,
                found: first,
            });
        }
        let header_record = parse_record(&mut cur, Placement::default())?;
        let header = FileHeader::from_record(&header_record)?;
        let mut plugin = Plugin {
            header,
            header_record,
            records: Vec::new(),
            groups: Vec::new(),
        };
        while !cur.is_empty() {
            let group = parse_group(&mut cur, &mut plugin.records, Placement::default())?;
            plugin.groups.push(group);
        }
        Ok(plugin)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.header_record.write(&mut out);
        for g in &self.groups {
            g.write(&self.records, &mut out);
        }
        out
    }

    /// Reorder existing masters or add new masters without changing the
    /// resolved identities represented by record ids and supported links.
    /// Existing masters cannot be removed because unknown subrecords may
    /// contain links to them.
    pub fn set_masters(&mut self, masters: Vec<String>) -> Result<()> {
        let old = &self.header.masters;
        if masters.len() > u8::MAX as usize {
            return Err(Error::Invalid(
                "plugin cannot have more than 255 masters".into(),
            ));
        }
        let mut normalized = std::collections::HashSet::new();
        for master in &masters {
            if !normalized.insert(master.to_ascii_lowercase()) {
                return Err(Error::Invalid(format!("duplicate master {master}")));
            }
        }
        for master in old {
            if !masters.iter().any(|new| new.eq_ignore_ascii_case(master)) {
                return Err(Error::Invalid(format!("cannot remove master {master}")));
            }
        }
        for record in &self.records {
            for subrecord in &record.subrecords {
                if is_form_id_field(record.tag, subrecord.tag) && subrecord.data.len() % 4 != 0 {
                    return Err(Error::Invalid(format!(
                        "{} {} FormID list has {} trailing bytes",
                        record.tag,
                        subrecord.tag,
                        subrecord.data.len() % 4
                    )));
                }
            }
        }
        if let Some(onam) = self
            .header_record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.tag == Tag::new(b"ONAM"))
        {
            if onam.data.len() % 4 != 0 {
                return Err(Error::Invalid(format!(
                    "TES4 ONAM FormID list has {} trailing bytes",
                    onam.data.len() % 4
                )));
            }
        }

        let relocate = |raw: u32| {
            let old_index = (raw >> 24) as usize;
            let object = raw & 0x00FF_FFFF;
            let new_index = old.get(old_index).and_then(|name| {
                masters
                    .iter()
                    .position(|new| new.eq_ignore_ascii_case(name))
            });
            let index = new_index.unwrap_or(masters.len());
            ((index as u32) << 24) | object
        };

        for record in &mut self.records {
            record.form_id = relocate(record.form_id);
            for subrecord in &mut record.subrecords {
                if is_form_id_field(record.tag, subrecord.tag) {
                    for bytes in subrecord.data.as_chunks_mut::<4>().0 {
                        let raw = u32::from_le_bytes(*bytes);
                        bytes.copy_from_slice(&relocate(raw).to_le_bytes());
                    }
                }
            }
        }
        for group in &mut self.groups {
            relocate_group(group, &relocate);
        }
        for subrecord in &mut self.header_record.subrecords {
            if subrecord.tag == Tag::new(b"ONAM") {
                for bytes in subrecord.data.as_chunks_mut::<4>().0 {
                    let raw = u32::from_le_bytes(*bytes);
                    bytes.copy_from_slice(&relocate(raw).to_le_bytes());
                }
            }
        }
        self.header.overridden_forms = self
            .header_record
            .find(Tag::new(b"ONAM"))
            .unwrap_or_default()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| u32::from_le_bytes(*bytes))
            .collect();
        self.header.masters = masters;
        rewrite_master_header(&mut self.header_record, &self.header.masters);
        Ok(())
    }

    /// SHA-256 of this plugin's serialized definition. This does not depend
    /// on its load slot or on another file's overrides.
    pub fn fingerprint(&self) -> [u8; 32] {
        Sha256::digest(self.to_bytes()).into()
    }

    /// Records of one signature, in file order.
    pub fn records_of(&self, tag: Tag) -> impl Iterator<Item = &Record> {
        self.records.iter().filter(move |r| r.tag == tag)
    }

    pub fn find_group_mut(&mut self, kind: GroupKind) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.kind == kind)
    }

    /// Append a record to a top-level group, creating the group if needed.
    /// Returns the record index. The placement is left as given.
    pub fn push_top(&mut self, record: Record) -> usize {
        let top = GroupKind::Top(record.tag);
        let idx = self.records.len();
        self.records.push(record);
        if self.find_group_mut(top).is_none() {
            self.groups.push(Group::new(top));
        }
        // CONTEXT: the group was just pushed above if it was missing.
        #[allow(clippy::expect_used)]
        self.find_group_mut(top)
            .expect("just created")
            .entries
            .push(Entry::Record(idx));
        idx
    }

    /// Append a record and return its index without placing it in a group.
    /// The caller adds an `Entry::Record` to the right group.
    pub fn push_record(&mut self, record: Record) -> usize {
        self.records.push(record);
        self.records.len() - 1
    }

    /// Count of records per signature, sorted by signature.
    pub fn counts(&self) -> Vec<(Tag, usize)> {
        let mut m = std::collections::BTreeMap::new();
        for r in &self.records {
            *m.entry(r.tag).or_insert(0) += 1;
        }
        m.into_iter().collect()
    }
}

fn relocate_group(group: &mut Group, relocate: &impl Fn(u32) -> u32) {
    match &mut group.kind {
        GroupKind::WorldChildren(id)
        | GroupKind::CellChildren(id)
        | GroupKind::TopicChildren(id)
        | GroupKind::CellPersistent(id)
        | GroupKind::CellTemporary(id)
        | GroupKind::CellVisibleDistant(id) => *id = relocate(*id),
        GroupKind::Top(_)
        | GroupKind::InteriorBlock(_)
        | GroupKind::InteriorSubBlock(_)
        | GroupKind::ExteriorBlock { .. }
        | GroupKind::ExteriorSubBlock { .. } => {}
    }
    for entry in &mut group.entries {
        if let Entry::Group(child) = entry {
            relocate_group(child, relocate);
        }
    }
}

fn rewrite_master_header(record: &mut Record, masters: &[String]) {
    let old_data: std::collections::HashMap<String, Vec<u8>> = record
        .subrecords
        .iter()
        .filter(|sub| sub.tag == Tag::new(b"MAST"))
        .map(|sub| crate::cursor::zstring(&sub.data).to_ascii_lowercase())
        .zip(
            record
                .subrecords
                .iter()
                .filter(|sub| sub.tag == Tag::new(b"DATA"))
                .map(|sub| sub.data.clone()),
        )
        .collect();
    let first_master = record
        .subrecords
        .iter()
        .position(|sub| sub.tag == Tag::new(b"MAST"))
        .unwrap_or(record.subrecords.len());
    let mut subrecords = Vec::with_capacity(record.subrecords.len() + masters.len() * 2);
    let mut inserted = false;
    for sub in record.subrecords.drain(..) {
        if sub.tag == Tag::new(b"MAST") || sub.tag == Tag::new(b"DATA") {
            continue;
        }
        if !inserted && subrecords.len() == first_master {
            append_masters(&mut subrecords, masters, &old_data);
            inserted = true;
        }
        subrecords.push(sub);
    }
    if !inserted {
        append_masters(&mut subrecords, masters, &old_data);
    }
    record.subrecords = subrecords;
}

fn append_masters(
    subrecords: &mut Vec<Subrecord>,
    masters: &[String],
    old_data: &std::collections::HashMap<String, Vec<u8>>,
) {
    for master in masters {
        let mut name = master.as_bytes().to_vec();
        name.push(0);
        subrecords.push(Subrecord::new(Tag::new(b"MAST"), name));
        subrecords.push(Subrecord::new(
            Tag::new(b"DATA"),
            old_data
                .get(&master.to_ascii_lowercase())
                .cloned()
                .unwrap_or_else(|| vec![0; 8]),
        ));
    }
}

pub(crate) fn is_form_id_field(record: Tag, field: Tag) -> bool {
    let pairs: &[(Tag, Tag)] = &[
        (tag::REFR, Tag(*b"NAME")),
        (tag::REFR, Tag(*b"XESP")),
        (tag::REFR, Tag(*b"XTEL")),
        (tag::REFR, Tag(*b"XCNT")),
        (tag::REFR, Tag(*b"XLKR")),
        (tag::REFR, Tag(*b"XOWN")),
        (tag::REFR, Tag(*b"XLCN")),
        (tag::REFR, Tag(*b"XLYR")),
        (tag::REFR, Tag(*b"XEZN")),
        (tag::REFR, Tag(*b"XEMI")),
        (tag::CELL, Tag(*b"LTMP")),
        (tag::CELL, Tag(*b"XCWT")),
        (tag::CELL, Tag(*b"XOWN")),
        (tag::CELL, Tag(*b"XLCN")),
        (tag::CELL, Tag(*b"XCLR")),
        (tag::CELL, Tag(*b"XCIM")),
        (tag::CELL, Tag(*b"XCMO")),
        (tag::CELL, Tag(*b"XCAS")),
        (tag::CELL, Tag(*b"XEZN")),
        (tag::CELL, Tag(*b"XCCM")),
        (tag::WRLD, Tag(*b"PNAM")),
        (tag::WRLD, Tag(*b"WCTR")),
        (tag::WRLD, Tag(*b"MNAM")),
        (tag::WRLD, Tag(*b"WNAM")),
        (tag::WRLD, Tag(*b"CNAM")),
        (tag::WRLD, Tag(*b"NAM2")),
        (tag::WRLD, Tag(*b"NAM3")),
        (tag::WRLD, Tag(*b"NAM4")),
        (tag::WRLD, Tag(*b"NAMA")),
        (tag::WRLD, Tag(*b"XLCN")),
        (tag::WRLD, Tag(*b"XEZN")),
        (tag::WRLD, Tag(*b"ZNAM")),
        (Tag(*b"LTEX"), Tag(*b"TNAM")),
        (Tag(*b"LTEX"), Tag(*b"GNAM")),
        (Tag(*b"LTEX"), Tag(*b"GNAM")),
        (Tag(*b"LCTN"), Tag(*b"PNAM")),
        (Tag(*b"LCTN"), Tag(*b"MNAM")),
        (Tag(*b"LCTN"), Tag(*b"RNAM")),
        (Tag(*b"NAVM"), Tag(*b"ONAM")),
        (Tag(*b"NAVM"), Tag(*b"PNAM")),
        (Tag(*b"NAVM"), Tag(*b"NNAM")),
        (Tag(*b"STAT"), Tag(*b"DNAM")),
        (Tag(*b"STAT"), Tag(*b"MNAM")),
    ];
    pairs.contains(&(record, field))
}

fn parse_record(cur: &mut Cursor<'_>, placement: Placement) -> Result<Record> {
    let offset = cur.offset();
    let rtag = cur.tag()?;
    let data_size = cur.u32()?;
    let flags = cur.u32()?;
    let form_id = cur.u32()?;
    let timestamp = cur.u16()?;
    let vcs = cur.u16()?;
    let form_version = cur.u16()?;
    let vcs2 = cur.u16()?;
    if cur.remaining() < data_size as usize {
        return Err(Error::RecordOverflow {
            at: offset,
            tag: rtag,
            size: data_size,
            available: cur.remaining(),
        });
    }
    let data_at = cur.offset();
    let raw = cur.take(data_size as usize)?;
    let inflated;
    let (data, base): (&[u8], usize) = if flags & flags::COMPRESSED != 0 {
        if raw.len() < 4 {
            return Err(Error::Truncated {
                at: data_at,
                need: 4,
                have: raw.len(),
            });
        }
        let expected = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let mut buf = Vec::with_capacity((expected as usize).min(MAX_INFLATE_PREALLOC));
        flate2::read::ZlibDecoder::new(&raw[4..])
            .take(u64::from(expected) + 1)
            .read_to_end(&mut buf)
            .map_err(|e| Error::Inflate {
                at: data_at,
                tag: rtag,
                reason: e.to_string(),
            })?;
        if buf.len() != expected as usize {
            return Err(Error::InflateSize {
                at: data_at,
                tag: rtag,
                expected,
                got: buf.len(),
            });
        }
        inflated = buf;
        (inflated.as_slice(), data_at)
    } else {
        (raw, data_at)
    };
    let subrecords = parse_subrecords(rtag, data, base)?;
    Ok(Record {
        tag: rtag,
        flags,
        form_id,
        timestamp,
        vcs,
        form_version,
        vcs2,
        subrecords,
        placement,
        offset,
    })
}

fn parse_subrecords(record: Tag, data: &[u8], base: usize) -> Result<Vec<Subrecord>> {
    let mut cur = Cursor::new(data, base);
    let mut out = Vec::new();
    let mut pending_size: Option<usize> = None;
    while !cur.is_empty() {
        let at = cur.offset();
        let stag = cur.tag()?;
        let declared = cur.u16()? as usize;
        if stag == tag::XXXX {
            let payload = cur.take(declared)?;
            if payload.len() != 4 {
                return Err(Error::SubrecordSize {
                    record,
                    tag: stag,
                    len: payload.len(),
                    expected: "exactly 4".into(),
                });
            }
            pending_size =
                Some(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize);
            if cur.is_empty() {
                return Err(Error::DanglingXxxx { at });
            }
            continue;
        }
        let size = pending_size.take().unwrap_or(declared);
        if cur.remaining() < size {
            return Err(Error::SubrecordOverflow {
                at,
                tag: stag,
                record,
                overshoot: size - cur.remaining(),
            });
        }
        let payload = cur.take(size)?;
        out.push(Subrecord {
            tag: stag,
            data: payload.to_vec(),
        });
    }
    Ok(out)
}

fn parse_group(cur: &mut Cursor<'_>, records: &mut Vec<Record>, outer: Placement) -> Result<Group> {
    let at = cur.offset();
    let gtag = cur.tag()?;
    if gtag != tag::GRUP {
        return Err(Error::Invalid(format!(
            "expected a GRUP at byte {at}, found record {gtag} outside any group"
        )));
    }
    let group_size = cur.u32()?;
    let label = {
        let l = cur.take(4)?;
        [l[0], l[1], l[2], l[3]]
    };
    let group_type = cur.i32()?;
    let timestamp = cur.u16()?;
    let vcs = cur.u16()?;
    let version = cur.u16()?;
    let unknown = cur.u16()?;
    if (group_size as usize) < HEADER_SIZE {
        return Err(Error::GroupTooSmall {
            at,
            size: group_size,
        });
    }
    let body_len = group_size as usize - HEADER_SIZE;
    if cur.remaining() < body_len {
        return Err(Error::GroupOverflow {
            at,
            size: group_size,
            available: cur.remaining() + HEADER_SIZE,
        });
    }
    let kind = GroupKind::from_raw(at, label, group_type)?;
    let placement = match kind {
        GroupKind::WorldChildren(id) => Placement {
            world: Some(id),
            cell: None,
            group: Some(kind),
        },
        GroupKind::CellChildren(id)
        | GroupKind::CellPersistent(id)
        | GroupKind::CellTemporary(id)
        | GroupKind::CellVisibleDistant(id) => Placement {
            world: outer.world,
            cell: Some(id),
            group: Some(kind),
        },
        _ => Placement {
            world: outer.world,
            cell: outer.cell,
            group: Some(kind),
        },
    };
    let mut body = cur.sub(body_len)?;
    let mut group = Group {
        kind,
        timestamp,
        vcs,
        version,
        unknown,
        entries: Vec::new(),
    };
    while !body.is_empty() {
        if body.peek_tag()? == tag::GRUP {
            let child = parse_group(&mut body, records, placement)?;
            group.entries.push(Entry::Group(child));
        } else {
            let rec = parse_record(&mut body, placement)?;
            records.push(rec);
            group.entries.push(Entry::Record(records.len() - 1));
        }
    }
    Ok(group)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> FileHeader {
        FileHeader {
            author: "test".into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_header_only_plugin_round_trips() {
        let p = Plugin::new(header());
        let bytes = p.to_bytes();
        let back = Plugin::parse(&bytes).unwrap();
        assert_eq!(back.header.author, "test");
        assert_eq!(back.header.next_object_id, 0x800);
        assert!(back.records.is_empty());
        assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn records_in_groups_round_trip_with_placement() {
        let mut p = Plugin::new(header());
        p.push_top(Record::new(tag::STAT, 0x801).zsub(tag::EDID, "Rock"));
        let cell = p.push_record(Record::new(tag::CELL, 0x802).zsub(tag::EDID, "Cave"));
        let refr =
            p.push_record(Record::new(tag::REFR, 0x803).sub(b"NAME", 0x801u32.to_le_bytes()));
        let mut temp = Group::new(GroupKind::CellTemporary(0x802));
        temp.entries.push(Entry::Record(refr));
        let mut children = Group::new(GroupKind::CellChildren(0x802));
        children.entries.push(Entry::Group(temp));
        let mut sub = Group::new(GroupKind::InteriorSubBlock(2));
        sub.entries.push(Entry::Record(cell));
        sub.entries.push(Entry::Group(children));
        let mut block = Group::new(GroupKind::InteriorBlock(2));
        block.entries.push(Entry::Group(sub));
        let mut top = Group::new(GroupKind::Top(tag::CELL));
        top.entries.push(Entry::Group(block));
        p.groups.push(top);

        let bytes = p.to_bytes();
        let back = Plugin::parse(&bytes).unwrap();
        assert_eq!(back.records.len(), 3);
        let r = back.records_of(tag::REFR).next().unwrap();
        assert_eq!(r.placement.cell, Some(0x802));
        assert_eq!(r.placement.group, Some(GroupKind::CellTemporary(0x802)));
        assert_eq!(back.to_bytes(), bytes);
        assert_eq!(
            back.counts(),
            vec![(tag::CELL, 1), (tag::REFR, 1), (tag::STAT, 1)]
        );
    }

    #[test]
    fn exterior_labels_keep_y_before_x() {
        let (label, kind) = GroupKind::ExteriorBlock { y: -1, x: 2 }.raw();
        assert_eq!(kind, 4);
        assert_eq!(label, [0xFF, 0xFF, 2, 0]);
        assert_eq!(
            GroupKind::from_raw(0, label, 4).unwrap(),
            GroupKind::ExteriorBlock { y: -1, x: 2 }
        );
    }

    #[test]
    fn compressed_records_are_inflated() {
        let mut p = Plugin::new(header());
        let payload = vec![7u8; 5000];
        p.push_top(Record::new(tag::STAT, 0x801).sub(b"DATA", payload.clone()));
        let plain = p.to_bytes();
        // Compress the STAT record by hand: header, then u32 size + zlib.
        let back = Plugin::parse(&plain).unwrap();
        let rec = &back.records[0];
        let mut body = Vec::new();
        Record {
            subrecords: rec.subrecords.clone(),
            ..Record::new(tag::STAT, 0x801)
        }
        .write(&mut body);
        let raw_body = &body[HEADER_SIZE..];
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, raw_body).unwrap();
        let z = enc.finish().unwrap();
        let mut data = (raw_body.len() as u32).to_le_bytes().to_vec();
        data.extend_from_slice(&z);
        let mut file = Vec::new();
        back.header_record.write(&mut file);
        let mut grp_body = Vec::new();
        grp_body.extend_from_slice(b"STAT");
        grp_body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        grp_body.extend_from_slice(&flags::COMPRESSED.to_le_bytes());
        grp_body.extend_from_slice(&0x801u32.to_le_bytes());
        grp_body.extend_from_slice(&[0u8; 4]);
        grp_body.extend_from_slice(&44u16.to_le_bytes());
        grp_body.extend_from_slice(&[0u8; 2]);
        grp_body.extend_from_slice(&data);
        file.extend_from_slice(b"GRUP");
        file.extend_from_slice(&((grp_body.len() + HEADER_SIZE) as u32).to_le_bytes());
        file.extend_from_slice(b"STAT");
        file.extend_from_slice(&[0u8; 12]);
        file.extend_from_slice(&grp_body);
        assert!(file.len() < plain.len(), "the compressed file is smaller");

        let parsed = Plugin::parse(&file).unwrap();
        assert!(parsed.records[0].is_compressed());
        assert_eq!(
            parsed.records[0].find(Tag::new(b"DATA")).unwrap(),
            payload.as_slice()
        );
    }

    #[test]
    fn garbage_in_a_compressed_record_is_an_inflate_error() {
        let mut p = Plugin::new(header());
        p.push_top(
            Record::new(tag::STAT, 0x801)
                .with_flags(flags::COMPRESSED)
                .sub(b"DATA", [1, 2, 3]),
        );
        // to_bytes clears the flag, so put it back and let the payload be garbage.
        let mut bytes = p.to_bytes();
        let hdr_len = {
            let mut h = Vec::new();
            p.header_record.write(&mut h);
            h.len()
        };
        let rec_at = hdr_len + HEADER_SIZE;
        let flags_at = rec_at + 8;
        bytes[flags_at..flags_at + 4].copy_from_slice(&flags::COMPRESSED.to_le_bytes());
        let err = Plugin::parse(&bytes).unwrap_err();
        assert!(
            matches!(err, Error::Inflate { tag, .. } if tag == tag::STAT),
            "{err}"
        );
    }

    /// Fuzz find (fuzz/artifacts/fuzz-esm/oom-b240fbaea1ba89b3e299e65bcab54895236456a8):
    /// a compressed record's leading "expected size" u32 is attacker
    /// controlled and was used to preallocate the inflate buffer before a
    /// single byte of the zlib stream was checked. A value near u32::MAX
    /// aborted the process with an out-of-memory allocation failure. The
    /// cap in `MAX_INFLATE_PREALLOC` bounds that allocation, so parsing
    /// must now fail cleanly instead.
    #[test]
    fn a_bogus_compressed_expected_size_does_not_preallocate_it() {
        let mut p = Plugin::new(header());
        p.push_top(
            Record::new(tag::STAT, 0x801)
                .with_flags(flags::COMPRESSED)
                .sub(b"DATA", [1, 2, 3, 4, 5, 6, 7, 8]),
        );
        let mut bytes = p.to_bytes();
        let hdr_len = {
            let mut h = Vec::new();
            p.header_record.write(&mut h);
            h.len()
        };
        let rec_at = hdr_len + HEADER_SIZE;
        let flags_at = rec_at + 8;
        bytes[flags_at..flags_at + 4].copy_from_slice(&flags::COMPRESSED.to_le_bytes());
        let data_at = rec_at + HEADER_SIZE;
        bytes[data_at..data_at + 4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let err = Plugin::parse(&bytes).unwrap_err();
        assert!(
            matches!(err, Error::Inflate { tag, .. } if tag == tag::STAT),
            "{err}"
        );
    }

    #[test]
    fn oversized_subrecords_use_xxxx() {
        let mut p = Plugin::new(header());
        let big = vec![3u8; 70_000];
        p.push_top(
            Record::new(tag::STAT, 0x801)
                .sub(b"DATA", big.clone())
                .zsub(tag::EDID, "after"),
        );
        let bytes = p.to_bytes();
        assert!(bytes.windows(4).any(|w| w == b"XXXX"));
        let back = Plugin::parse(&bytes).unwrap();
        assert_eq!(
            back.records[0].find(Tag::new(b"DATA")).unwrap(),
            big.as_slice()
        );
        assert_eq!(back.records[0].editor_id().as_deref(), Some("after"));
    }

    #[test]
    fn a_truncated_file_names_the_offset() {
        let mut p = Plugin::new(header());
        p.push_top(Record::new(tag::STAT, 0x801).zsub(tag::EDID, "Rock"));
        let bytes = p.to_bytes();
        let cut = &bytes[..bytes.len() - 5];
        let err = Plugin::parse(cut).unwrap_err();
        assert!(matches!(err, Error::GroupOverflow { .. }), "{err}");
        // Cut inside the header record instead.
        let err = Plugin::parse(&bytes[..10]).unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }), "{err}");
    }

    #[test]
    fn a_group_smaller_than_its_header_is_rejected() {
        let mut p = Plugin::new(header());
        p.push_top(Record::new(tag::STAT, 0x801));
        let mut bytes = p.to_bytes();
        let hdr_len = {
            let mut h = Vec::new();
            p.header_record.write(&mut h);
            h.len()
        };
        bytes[hdr_len + 4..hdr_len + 8].copy_from_slice(&3u32.to_le_bytes());
        assert!(matches!(
            Plugin::parse(&bytes).unwrap_err(),
            Error::GroupTooSmall { .. }
        ));
        bytes[hdr_len + 4..hdr_len + 8].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert!(matches!(
            Plugin::parse(&bytes).unwrap_err(),
            Error::GroupOverflow { .. }
        ));
    }

    #[test]
    fn a_file_without_tes4_is_rejected() {
        let err = Plugin::parse(b"GRUP\0\0\0\0\0\0\0\0").unwrap_err();
        assert_eq!(
            err,
            Error::NoHeader {
                at: 0,
                found: tag::GRUP
            }
        );
        assert!(matches!(
            Plugin::parse(b"TE").unwrap_err(),
            Error::Truncated { .. }
        ));
    }

    #[test]
    fn a_subrecord_past_its_record_is_reported() {
        let mut p = Plugin::new(header());
        p.push_top(Record::new(tag::STAT, 0x801).sub(b"DATA", [1, 2, 3, 4]));
        let mut bytes = p.to_bytes();
        let n = bytes.len();
        // The DATA size field sits 4 bytes before the payload at the end.
        bytes[n - 6..n - 4].copy_from_slice(&200u16.to_le_bytes());
        let err = Plugin::parse(&bytes).unwrap_err();
        assert!(
            matches!(err, Error::SubrecordOverflow { overshoot: 196, .. }),
            "{err}"
        );
    }
}
