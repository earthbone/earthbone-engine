// SPDX-License-Identifier: GPL-3.0-or-later

//! Load-order aware reader for TES4 family plugins, targeting Skyrim SE.
//!
//! Three layers:
//!
//! 1. `plugin`: one file as a group tree of records, each record its list of
//!    subrecords. Compressed records are inflated, `XXXX` size markers are
//!    folded away, and every record can be written back.
//! 2. `load_order`: several plugins in order, with FormIDs resolved through
//!    each file's master list into `GlobalFormId`s, and a form table where
//!    the last file to carry a record wins.
//! 3. `records`: typed views of CELL, REFR, WRLD, LAND, NAVM and STAT built
//!    from a record on demand.
//! 4. `cell_index`: cell to placed objects, land and navmeshes, and
//!    worldspace grid to cell, built once from the winning placements.
//! 5. `dirty`: per plugin, what xEdit's cleaning would touch, counted.
//!
//! ```no_run
//! use soe_esm::{LoadOrder, Plugin, records::cell::Cell, tag};
//!
//! let mut order = LoadOrder::new();
//! for name in ["Skyrim.esm", "Update.esm"] {
//!     let bytes = std::fs::read(format!("Data/{name}")).unwrap();
//!     order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
//! }
//! for (id, rec) in order.winners_of(tag::CELL) {
//!     let cell = Cell::from_record(rec, order.file(id.file as usize).plugin.header.is_localized()).unwrap();
//!     println!("{id} {:?} interior={}", cell.editor_id, cell.is_interior());
//! }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod cell_index;
pub mod cursor;
pub mod dirty;
pub mod error;
pub mod load_order;
pub mod plugin;
pub mod records;
pub mod renumber;
pub mod strings;
pub mod tag;

pub use cell_index::{CellIndex, GridCell};
pub use dirty::{DirtyReport, PluginDirt};
pub use error::{Error, Result};
pub use load_order::{GlobalFormId, LoadOrder, LoadedFile, RecordRef, Slot, CREATED_FILE};
pub use plugin::{Entry, FileHeader, Group, GroupKind, Placement, Plugin, Record, Subrecord};
pub use tag::Tag;

/// Read and parse one plugin file.
pub fn read_plugin(path: impl AsRef<std::path::Path>) -> std::io::Result<Result<Plugin>> {
    let bytes = std::fs::read(path)?;
    Ok(Plugin::parse(&bytes))
}

/// Read and parse one plugin, reusing its parsed tree from `cache_dir` when
/// the canonical path and file metadata match.
///
/// Cache failures are ignored. The source plugin is still parsed, and only a
/// successful parse can replace a cache entry.
pub fn read_plugin_cached(
    path: impl AsRef<std::path::Path>,
    cache_dir: impl AsRef<std::path::Path>,
) -> std::io::Result<Result<Plugin>> {
    use std::time::UNIX_EPOCH;

    let canonical_path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&canonical_path)?;
    let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let path_identity = cache_path_identity(&canonical_path);
    let size = metadata.len();
    let modified_secs = modified.as_secs();
    let modified_nanos = modified.subsec_nanos();
    let cache_path = cache_file_path(
        cache_dir.as_ref(),
        &path_identity,
        size,
        modified_secs,
        modified_nanos,
    );

    if let Ok(bytes) = std::fs::read(&cache_path) {
        if let Some(plugin) =
            decode_plugin_cache(&bytes, &path_identity, size, modified_secs, modified_nanos)
        {
            return Ok(Ok(plugin));
        }
    }

    let bytes = std::fs::read(&canonical_path)?;
    let parsed = Plugin::parse(&bytes);
    if let Ok(plugin) = &parsed {
        if let Some(encoded) =
            encode_plugin_cache(&path_identity, size, modified_secs, modified_nanos, plugin)
        {
            write_plugin_cache(&cache_path, &encoded);
        }
    }
    Ok(parsed)
}

const PLUGIN_CACHE_MAGIC: &[u8; 8] = b"SOEPC001";
const PLUGIN_CACHE_FORMAT_VERSION: u32 = 1;

fn cache_path_identity(path: &std::path::Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
    #[cfg(not(any(unix, windows)))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

fn cache_file_path(
    cache_dir: &std::path::Path,
    path_identity: &[u8],
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
) -> std::path::PathBuf {
    use sha2::{Digest, Sha256};

    let mut digest = Sha256::new();
    digest.update(path_identity);
    digest.update(size.to_le_bytes());
    digest.update(modified_secs.to_le_bytes());
    digest.update(modified_nanos.to_le_bytes());
    digest.update(PLUGIN_CACHE_FORMAT_VERSION.to_le_bytes());
    cache_dir.join(format!("{:x}.soe-plugin-cache", digest.finalize()))
}

fn encode_plugin_cache(
    path_identity: &[u8],
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
    plugin: &Plugin,
) -> Option<Vec<u8>> {
    use sha2::{Digest, Sha256};

    let mut writer = CacheWriter::default();
    writer.bytes(path_identity)?;
    writer.u64(size);
    writer.u64(modified_secs);
    writer.u32(modified_nanos);
    writer.u32(PLUGIN_CACHE_FORMAT_VERSION);
    writer.plugin(plugin)?;
    let payload = writer.bytes_out;
    let length = u64::try_from(payload.len()).ok()?;
    let digest = Sha256::digest(&payload);
    let mut cache = Vec::with_capacity(48 + payload.len());
    cache.extend_from_slice(PLUGIN_CACHE_MAGIC);
    cache.extend_from_slice(&length.to_le_bytes());
    cache.extend_from_slice(&digest);
    cache.extend_from_slice(&payload);
    Some(cache)
}

fn decode_plugin_cache(
    cache: &[u8],
    path_identity: &[u8],
    size: u64,
    modified_secs: u64,
    modified_nanos: u32,
) -> Option<Plugin> {
    use sha2::{Digest, Sha256};

    if cache.len() < 48 || cache.get(..8)? != PLUGIN_CACHE_MAGIC {
        return None;
    }
    let payload_len =
        usize::try_from(u64::from_le_bytes(cache.get(8..16)?.try_into().ok()?)).ok()?;
    let payload = cache.get(48..)?;
    if payload.len() != payload_len || Sha256::digest(payload).as_slice() != cache.get(16..48)? {
        return None;
    }
    let mut reader = CacheReader::new(payload);
    let cached_path = reader.bytes()?;
    let cached_size = reader.u64()?;
    let cached_secs = reader.u64()?;
    let cached_nanos = reader.u32()?;
    let version = reader.u32()?;
    if cached_path != path_identity
        || cached_size != size
        || cached_secs != modified_secs
        || cached_nanos != modified_nanos
        || version != PLUGIN_CACHE_FORMAT_VERSION
    {
        return None;
    }
    let plugin = reader.plugin()?;
    reader.is_empty().then_some(plugin)
}

#[derive(Default)]
struct CacheWriter {
    bytes_out: Vec<u8>,
}

impl CacheWriter {
    fn u8(&mut self, value: u8) {
        self.bytes_out.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes_out.extend_from_slice(&value.to_le_bytes());
    }

    fn i16(&mut self, value: i16) {
        self.bytes_out.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes_out.extend_from_slice(&value.to_le_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.bytes_out.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes_out.extend_from_slice(&value.to_le_bytes());
    }

    fn sized_count(&mut self, count: usize) -> Option<()> {
        self.u32(u32::try_from(count).ok()?);
        Some(())
    }

    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        self.u64(u64::try_from(bytes.len()).ok()?);
        self.bytes_out.extend_from_slice(bytes);
        Some(())
    }

    fn string(&mut self, text: &str) -> Option<()> {
        self.bytes(text.as_bytes())
    }

    fn tag(&mut self, value: crate::Tag) {
        self.bytes_out.extend_from_slice(value.as_bytes());
    }

    fn option_u32(&mut self, value: Option<u32>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.u32(value);
            }
            None => self.u8(0),
        }
    }

    fn group_kind(&mut self, kind: GroupKind) {
        match kind {
            GroupKind::Top(tag) => {
                self.u8(0);
                self.tag(tag);
            }
            GroupKind::WorldChildren(value) => self.tagged_u32(1, value),
            GroupKind::InteriorBlock(value) => self.tagged_i32(2, value),
            GroupKind::InteriorSubBlock(value) => self.tagged_i32(3, value),
            GroupKind::ExteriorBlock { y, x } => self.tagged_grid(4, y, x),
            GroupKind::ExteriorSubBlock { y, x } => self.tagged_grid(5, y, x),
            GroupKind::CellChildren(value) => self.tagged_u32(6, value),
            GroupKind::TopicChildren(value) => self.tagged_u32(7, value),
            GroupKind::CellPersistent(value) => self.tagged_u32(8, value),
            GroupKind::CellTemporary(value) => self.tagged_u32(9, value),
            GroupKind::CellVisibleDistant(value) => self.tagged_u32(10, value),
        }
    }

    fn tagged_u32(&mut self, kind: u8, value: u32) {
        self.u8(kind);
        self.u32(value);
    }

    fn tagged_i32(&mut self, kind: u8, value: i32) {
        self.u8(kind);
        self.i32(value);
    }

    fn tagged_grid(&mut self, kind: u8, y: i16, x: i16) {
        self.u8(kind);
        self.i16(y);
        self.i16(x);
    }

    fn record(&mut self, record: &Record) -> Option<()> {
        self.tag(record.tag);
        self.u32(record.flags);
        self.u32(record.form_id);
        self.u16(record.timestamp);
        self.u16(record.vcs);
        self.u16(record.form_version);
        self.u16(record.vcs2);
        self.sized_count(record.subrecords.len())?;
        for subrecord in &record.subrecords {
            self.tag(subrecord.tag);
            self.bytes(&subrecord.data)?;
        }
        self.option_u32(record.placement.world);
        self.option_u32(record.placement.cell);
        match record.placement.group {
            Some(group) => {
                self.u8(1);
                self.group_kind(group);
            }
            None => self.u8(0),
        }
        self.u64(u64::try_from(record.offset).ok()?);
        Some(())
    }

    fn group(&mut self, group: &Group, depth: usize) -> Option<()> {
        if depth > 256 {
            return None;
        }
        self.group_kind(group.kind);
        self.u16(group.timestamp);
        self.u16(group.vcs);
        self.u16(group.version);
        self.u16(group.unknown);
        self.sized_count(group.entries.len())?;
        for entry in &group.entries {
            match entry {
                Entry::Record(index) => {
                    self.u8(0);
                    self.u64(u64::try_from(*index).ok()?);
                }
                Entry::Group(group) => {
                    self.u8(1);
                    self.group(group, depth + 1)?;
                }
            }
        }
        Some(())
    }

    fn plugin(&mut self, plugin: &Plugin) -> Option<()> {
        let header = &plugin.header;
        self.u32(header.flags);
        self.u32(header.version.to_bits());
        self.u32(header.record_count);
        self.u32(header.next_object_id);
        self.string(&header.author)?;
        self.string(&header.description)?;
        self.sized_count(header.masters.len())?;
        for master in &header.masters {
            self.string(master)?;
        }
        self.sized_count(header.overridden_forms.len())?;
        for form in &header.overridden_forms {
            self.u32(*form);
        }
        self.option_u32(header.interior_cell_count);
        self.record(&plugin.header_record)?;
        self.sized_count(plugin.records.len())?;
        for record in &plugin.records {
            self.record(record)?;
        }
        self.sized_count(plugin.groups.len())?;
        for group in &plugin.groups {
            self.group(group, 0)?;
        }
        Some(())
    }
}

struct CacheReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

enum CacheOptionU32 {
    Invalid,
    Absent,
    Present(u32),
}

impl<'a> CacheReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(length)?;
        let out = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(out)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(*self.take(1)?.first()?)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i16(&mut self) -> Option<i16> {
        Some(i16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn count(&mut self) -> Option<usize> {
        let count = usize::try_from(self.u32()?).ok()?;
        (count <= self.bytes.len().saturating_sub(self.offset)).then_some(count)
    }

    fn bytes(&mut self) -> Option<Vec<u8>> {
        let length = usize::try_from(self.u64()?).ok()?;
        Some(self.take(length)?.to_vec())
    }

    fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?).ok()
    }

    fn tag(&mut self) -> Option<crate::Tag> {
        Some(crate::Tag(self.take(4)?.try_into().ok()?))
    }

    fn option_u32(&mut self) -> CacheOptionU32 {
        match self.u8() {
            Some(0) => CacheOptionU32::Absent,
            Some(1) => self
                .u32()
                .map_or(CacheOptionU32::Invalid, CacheOptionU32::Present),
            _ => CacheOptionU32::Invalid,
        }
    }

    fn group_kind(&mut self) -> Option<GroupKind> {
        Some(match self.u8()? {
            0 => GroupKind::Top(self.tag()?),
            1 => GroupKind::WorldChildren(self.u32()?),
            2 => GroupKind::InteriorBlock(self.i32()?),
            3 => GroupKind::InteriorSubBlock(self.i32()?),
            4 => GroupKind::ExteriorBlock {
                y: self.i16()?,
                x: self.i16()?,
            },
            5 => GroupKind::ExteriorSubBlock {
                y: self.i16()?,
                x: self.i16()?,
            },
            6 => GroupKind::CellChildren(self.u32()?),
            7 => GroupKind::TopicChildren(self.u32()?),
            8 => GroupKind::CellPersistent(self.u32()?),
            9 => GroupKind::CellTemporary(self.u32()?),
            10 => GroupKind::CellVisibleDistant(self.u32()?),
            _ => return None,
        })
    }

    fn record(&mut self) -> Option<Record> {
        let tag = self.tag()?;
        let flags = self.u32()?;
        let form_id = self.u32()?;
        let timestamp = self.u16()?;
        let vcs = self.u16()?;
        let form_version = self.u16()?;
        let vcs2 = self.u16()?;
        let count = self.count()?;
        let mut subrecords = Vec::new();
        subrecords.try_reserve(count).ok()?;
        for _ in 0..count {
            subrecords.push(Subrecord {
                tag: self.tag()?,
                data: self.bytes()?,
            });
        }
        let world = match self.option_u32() {
            CacheOptionU32::Invalid => return None,
            CacheOptionU32::Absent => None,
            CacheOptionU32::Present(value) => Some(value),
        };
        let cell = match self.option_u32() {
            CacheOptionU32::Invalid => return None,
            CacheOptionU32::Absent => None,
            CacheOptionU32::Present(value) => Some(value),
        };
        let group = match self.u8()? {
            0 => None,
            1 => Some(self.group_kind()?),
            _ => return None,
        };
        let offset = usize::try_from(self.u64()?).ok()?;
        Some(Record {
            tag,
            flags,
            form_id,
            timestamp,
            vcs,
            form_version,
            vcs2,
            subrecords,
            placement: Placement { world, cell, group },
            offset,
        })
    }

    fn group(&mut self, depth: usize) -> Option<Group> {
        if depth > 256 {
            return None;
        }
        let kind = self.group_kind()?;
        let timestamp = self.u16()?;
        let vcs = self.u16()?;
        let version = self.u16()?;
        let unknown = self.u16()?;
        let count = self.count()?;
        let mut entries = Vec::new();
        entries.try_reserve(count).ok()?;
        for _ in 0..count {
            entries.push(match self.u8()? {
                0 => Entry::Record(usize::try_from(self.u64()?).ok()?),
                1 => Entry::Group(self.group(depth + 1)?),
                _ => return None,
            });
        }
        Some(Group {
            kind,
            timestamp,
            vcs,
            version,
            unknown,
            entries,
        })
    }

    fn plugin(&mut self) -> Option<Plugin> {
        let flags = self.u32()?;
        let version = f32::from_bits(self.u32()?);
        let record_count = self.u32()?;
        let next_object_id = self.u32()?;
        let author = self.string()?;
        let description = self.string()?;
        let master_count = self.count()?;
        let mut masters = Vec::new();
        masters.try_reserve(master_count).ok()?;
        for _ in 0..master_count {
            masters.push(self.string()?);
        }
        let override_count = self.count()?;
        let mut overridden_forms = Vec::new();
        overridden_forms.try_reserve(override_count).ok()?;
        for _ in 0..override_count {
            overridden_forms.push(self.u32()?);
        }
        let interior_cell_count = match self.option_u32() {
            CacheOptionU32::Invalid => return None,
            CacheOptionU32::Absent => None,
            CacheOptionU32::Present(value) => Some(value),
        };
        let header_record = self.record()?;
        let records_len = self.count()?;
        let mut records = Vec::new();
        records.try_reserve(records_len).ok()?;
        for _ in 0..records_len {
            records.push(self.record()?);
        }
        let group_count = self.count()?;
        let mut groups = Vec::new();
        groups.try_reserve(group_count).ok()?;
        for _ in 0..group_count {
            groups.push(self.group(0)?);
        }
        Some(Plugin {
            header: FileHeader {
                flags,
                version,
                record_count,
                next_object_id,
                author,
                description,
                masters,
                overridden_forms,
                interior_cell_count,
            },
            header_record,
            records,
            groups,
        })
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn write_plugin_cache(path: &std::path::Path, bytes: &[u8]) {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let temp_path = path.with_extension(format!(
        "soe-plugin-cache-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    if std::fs::write(&temp_path, bytes).is_ok()
        && std::fs::rename(&temp_path, path).is_err()
        && std::fs::remove_file(path).is_ok()
    {
        let _ = std::fs::rename(&temp_path, path);
    }
    let _ = std::fs::remove_file(temp_path);
}
