// SPDX-License-Identifier: GPL-3.0-or-later
// Ported from linux-mod-organizer `src/bsa/parser.rs`,
// Copyright (C) 2026 Dr. Baklava (cd4u2b0z), GPL-3.0-or-later.

//! BSA (v103 Oblivion, v104 Fallout 3 and Skyrim LE, v105 Skyrim SE) and BA2
//! (GNRL and DX10) readers.
//!
//! Two entry points. [`index_archive`] reads the header and tables only, which
//! is what a tool listing a hundred archives wants: a real 246 MB archive
//! describes its 468 files in 20 KB. [`Archive::open`] keeps only that index
//! and reads each payload from disk on demand. [`Archive::from_bytes`] retains
//! its in-memory API for callers that already own an archive buffer.

use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::error::{Error, Result};
use crate::hash::{file_name_hash, folder_hash};
use crate::normalize;

#[derive(Debug, Clone)]
pub struct ArchiveIndex {
    pub format: ArchiveFormat,
    pub file_count: u32,
    /// Forward-slash paths as stored, in archive order.
    pub files: Vec<String>,
    /// Parallel to `files`: where each file's bytes are.
    pub records: Vec<FileRecord>,
}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub offset: u64,
    pub raw_size: u32,
    pub compressed: bool,
    /// For compressed files the uncompressed size. BA2 states it in the
    /// record. BSA stores it at the file's own offset, so an index taken off
    /// disk leaves it 0 and only an in-memory index fills it.
    pub uncompressed_size: u32,
    /// BSA with the embed-names flag: `offset` and `raw_size` cover a block
    /// that starts with a length-prefixed path, then the size word when
    /// compressed, then the payload. Skyrim SE texture archives do this.
    pub name_embedded: bool,
    /// BA2 DX10 textures: the chunk layout the DDS is rebuilt from.
    pub dds_info: Option<DdsInfo>,
}

#[derive(Debug, Clone)]
pub struct DdsInfo {
    pub height: u16,
    pub width: u16,
    pub num_mips: u8,
    pub dxgi_format: u8,
    pub chunks: Vec<DdsChunk>,
}

#[derive(Debug, Clone)]
pub struct DdsChunk {
    pub offset: u64,
    pub packed_size: u32,
    pub unpacked_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Bsa(BsaVersion),
    Ba2(Ba2Type),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsaVersion {
    /// v103
    Oblivion,
    /// v104: Fallout 3, New Vegas, Skyrim LE
    Fo3Skyrim,
    /// v105: Skyrim SE and AE
    SkyrimSe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ba2Type {
    General,
    Dx10,
}

impl std::fmt::Display for ArchiveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchiveFormat::Bsa(v) => {
                let ver = match v {
                    BsaVersion::Oblivion => "v103/Oblivion",
                    BsaVersion::Fo3Skyrim => "v104/FO3+SkyrimLE",
                    BsaVersion::SkyrimSe => "v105/SkyrimSE",
                };
                write!(f, "BSA ({ver})")
            }
            ArchiveFormat::Ba2(t) => {
                let typ = match t {
                    Ba2Type::General => "General",
                    Ba2Type::Dx10 => "DX10/Textures",
                };
                write!(f, "BA2 ({typ})")
            }
        }
    }
}

const BSA_MAGIC: &[u8; 4] = b"BSA\0";
const BA2_MAGIC: &[u8; 4] = b"BTDX";

const BSA_FLAG_INCLUDE_DIR_NAMES: u32 = 0x01;
const BSA_FLAG_INCLUDE_FILE_NAMES: u32 = 0x02;
const BSA_FLAG_COMPRESSED: u32 = 0x04;
const BSA_FLAG_EMBED_FILE_NAMES: u32 = 0x100;
const BSA_SIZE_COMPRESSION_BIT: u32 = 1 << 30;
const BA2_DX10_CHUNK_SENTINEL: u32 = 0xBAAD_F00D;

/// Index an archive off disk, reading only the header and tables.
///
/// `uncompressed_size` is left 0 for compressed BSA entries. It lives at the
/// file's own offset, out in the payload, and only a read needs it.
pub fn index_archive(path: &Path) -> Result<ArchiveIndex> {
    let file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len < 8 {
        return Err(Error::format("Archive too small", path));
    }
    let mut r = std::io::BufReader::new(file);
    index_from(&mut r, len, path, false)
}

/// Index an archive already held in memory. Fills `uncompressed_size` for
/// compressed BSA entries, since the bytes are there anyway.
pub fn index_archive_in(data: &[u8], path: &Path) -> Result<ArchiveIndex> {
    if data.len() < 8 {
        return Err(Error::format("Archive too small", path));
    }
    let mut cursor = Cursor::new(data);
    index_from(&mut cursor, data.len() as u64, path, true)
}

pub(crate) fn index_from<R: Read + Seek>(
    r: &mut R,
    len: u64,
    path: &Path,
    peek_payload: bool,
) -> Result<ArchiveIndex> {
    let mut magic = [0u8; 4];
    r.read_exact(&mut magic)?;

    if &magic == BSA_MAGIC {
        parse_bsa(r, len, path, peek_payload)
    } else if &magic == BA2_MAGIC {
        parse_ba2(r, len, path)
    } else {
        Err(Error::format(
            format!("Unknown archive format (magic: {magic:?})"),
            path,
        ))
    }
}

enum ArchiveStorage {
    Memory(Vec<u8>),
    Disk {
        file: Mutex<std::fs::File>,
        len: u64,
    },
}

struct IndexReader<'a> {
    file: &'a std::fs::File,
    path: &'a Path,
    read_time: std::time::Duration,
    seek_time: std::time::Duration,
}

impl Read for IndexReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let started = std::time::Instant::now();
        let result = self.file.read(buffer);
        self.read_time += started.elapsed();
        result
    }
}

impl Seek for IndexReader<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let started = std::time::Instant::now();
        let result = self.file.seek(position);
        self.seek_time += started.elapsed();
        result
    }
}

impl Drop for IndexReader<'_> {
    fn drop(&mut self) {
        let asset = self.path.display().to_string();
        soe_trace::LoadTrace::accumulated(
            "archive_index_read",
            soe_trace::LoadCause::IoWait,
            &asset,
            self.read_time,
        );
        soe_trace::LoadTrace::accumulated(
            "archive_index_seek",
            soe_trace::LoadCause::IoWait,
            &asset,
            self.seek_time,
        );
    }
}

/// An archive index with a case-insensitive path lookup and on-demand payloads.
pub struct Archive {
    path: PathBuf,
    storage: ArchiveStorage,
    index: ArchiveIndex,
    /// Normalised path to record position.
    lookup: HashMap<String, usize>,
}

// CONTEXT: backing storage and the lookup are too large to print, so this
// intentionally shows a summary rather than every field.
#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for Archive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Archive")
            .field("path", &self.path)
            .field("format", &self.index.format)
            .field("files", &self.index.files.len())
            .finish()
    }
}

impl Archive {
    pub fn open(path: &Path) -> Result<Self> {
        let _span = soe_trace::Span::new("bsa.archive_open", "bsa");
        soe_trace::debug!(target: "bsa", system = "bsa", archive = %path.display(), "opening archive");
        let file = {
            let _io = soe_trace::LoadScope::new(
                "archive_open",
                soe_trace::LoadCause::IoWait,
                path.display().to_string(),
            );
            std::fs::File::open(path)?
        };
        let len = file.metadata()?.len();
        if len < 8 {
            return Err(Error::format("Archive too small", path));
        }
        let index = {
            let _index = soe_trace::LoadScope::new(
                "archive_index",
                soe_trace::LoadCause::Parse,
                path.display().to_string(),
            );
            let mut reader = std::io::BufReader::new(IndexReader {
                file: &file,
                path,
                read_time: std::time::Duration::ZERO,
                seek_time: std::time::Duration::ZERO,
            });
            index_from(&mut reader, len, path, false)?
        };
        Ok(Self::with_storage(
            path,
            index,
            ArchiveStorage::Disk {
                file: Mutex::new(file),
                len,
            },
        ))
    }

    /// `path` only names the archive in errors.
    pub fn from_bytes(path: &Path, data: Vec<u8>) -> Result<Self> {
        let index = index_archive_in(&data, path)?;
        Ok(Self::with_storage(
            path,
            index,
            ArchiveStorage::Memory(data),
        ))
    }

    fn with_storage(path: &Path, index: ArchiveIndex, storage: ArchiveStorage) -> Self {
        let mut lookup = HashMap::with_capacity(index.records.len());
        for (i, r) in index.records.iter().enumerate() {
            // First occurrence wins, matching the game's own lookup order.
            lookup.entry(normalize(&r.path)).or_insert(i);
        }
        Archive {
            path: path.to_path_buf(),
            storage,
            index,
            lookup,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn index(&self) -> &ArchiveIndex {
        &self.index
    }

    #[must_use]
    pub fn format(&self) -> ArchiveFormat {
        self.index.format
    }

    /// Paths as stored, in archive order.
    #[must_use]
    pub fn files(&self) -> &[String] {
        &self.index.files
    }

    /// Record position for a path, case-insensitive, either slash.
    #[must_use]
    pub fn find(&self, path: &str) -> Option<usize> {
        self.lookup.get(&normalize(path)).copied()
    }

    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    /// The file's bytes, decompressed. A DX10 texture comes back as a whole
    /// DDS file with its header rebuilt.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let i = self.find(path).ok_or_else(|| Error::NotFound {
            name: path.to_string(),
        })?;
        self.read_record(i)
    }

    /// Same as [`Archive::read`] by record position.
    pub fn read_record(&self, i: usize) -> Result<Vec<u8>> {
        let record = self.index.records.get(i).ok_or_else(|| Error::NotFound {
            name: i.to_string(),
        })?;
        let _payload = (record.compressed || record.dds_info.is_some()).then(|| {
            soe_trace::LoadScope::new(
                "archive_payload",
                soe_trace::LoadCause::Decode,
                format!("{}:{}", self.path.display(), record.path),
            )
        });
        match &self.storage {
            ArchiveStorage::Memory(data) => read_record(data, self.index.format, record),
            ArchiveStorage::Disk { file, len } => {
                read_disk_record(file, *len, self.index.format, record, &self.path)
            }
        }
    }
}

fn read_disk_range(
    file: &Mutex<std::fs::File>,
    len: u64,
    name: &str,
    offset: u64,
    size: u64,
    archive: &Path,
) -> Result<Vec<u8>> {
    let _io = soe_trace::LoadScope::new(
        "archive_read",
        soe_trace::LoadCause::IoWait,
        format!("{}:{name}", archive.display()),
    );
    if offset
        .checked_add(size)
        .as_ref()
        .is_none_or(|end| *end > len)
    {
        return Err(Error::Truncated {
            name: name.to_string(),
            offset,
            size,
        });
    }
    let size = usize::try_from(size).map_err(|_| Error::Truncated {
        name: name.to_string(),
        offset,
        size,
    })?;
    let mut bytes = vec![0; size];
    let mut file = file
        .lock()
        .map_err(|_| Error::Io(std::io::Error::other("archive read lock poisoned")))?;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(&mut bytes).map_err(|source| {
        if source.kind() == std::io::ErrorKind::UnexpectedEof {
            Error::Truncated {
                name: name.to_string(),
                offset,
                size: size as u64,
            }
        } else {
            Error::Io(source)
        }
    })?;
    Ok(bytes)
}

fn read_disk_record(
    file: &Mutex<std::fs::File>,
    len: u64,
    format: ArchiveFormat,
    record: &FileRecord,
    archive: &Path,
) -> Result<Vec<u8>> {
    if let Some(dds) = &record.dds_info {
        return rebuild_dds_from_disk(file, len, &record.path, dds, archive);
    }
    let size = if record.name_embedded || record.compressed {
        record.raw_size
    } else if record.uncompressed_size > 0 {
        record.uncompressed_size
    } else {
        record.raw_size
    };
    let bytes = read_disk_range(
        file,
        len,
        &record.path,
        record.offset,
        u64::from(size),
        archive,
    )?;
    let mut memory_record = record.clone();
    memory_record.offset = 0;
    if memory_record.compressed
        && !memory_record.name_embedded
        && matches!(format, ArchiveFormat::Bsa(_))
        && memory_record.uncompressed_size == 0
    {
        let prefix_offset = record
            .offset
            .checked_sub(4)
            .ok_or_else(|| Error::Truncated {
                name: record.path.clone(),
                offset: record.offset,
                size: 4,
            })?;
        let prefix_bytes = read_disk_range(file, len, &record.path, prefix_offset, 4, archive)?;
        let prefix_bytes: [u8; 4] = prefix_bytes.try_into().map_err(|_| Error::Truncated {
            name: record.path.clone(),
            offset: prefix_offset,
            size: 4,
        })?;
        memory_record.uncompressed_size = u32::from_le_bytes(prefix_bytes);
    }
    read_record(&bytes, format, &memory_record)
}

fn is_sse(format: ArchiveFormat) -> bool {
    format == ArchiveFormat::Bsa(BsaVersion::SkyrimSe)
}

fn slice<'a>(data: &'a [u8], name: &str, offset: u64, size: u64) -> Result<&'a [u8]> {
    let end = offset.checked_add(size).filter(|e| *e <= data.len() as u64);
    match end {
        Some(end) => Ok(&data[offset as usize..end as usize]),
        None => Err(Error::Truncated {
            name: name.to_string(),
            offset,
            size,
        }),
    }
}

/// The bytes of one record out of a whole archive held in memory.
pub fn read_record(data: &[u8], format: ArchiveFormat, record: &FileRecord) -> Result<Vec<u8>> {
    let _decode = (record.compressed || record.dds_info.is_some()).then(|| {
        soe_trace::LoadScope::new(
            "archive_decode",
            soe_trace::LoadCause::Decode,
            record.path.clone(),
        )
    });
    if let Some(dds) = &record.dds_info {
        return rebuild_dds(data, &record.path, dds);
    }
    if record.name_embedded {
        let block = slice(
            data,
            &record.path,
            record.offset,
            u64::from(record.raw_size),
        )?;
        let name_len = *block.first().ok_or_else(|| Error::Truncated {
            name: record.path.clone(),
            offset: record.offset,
            size: u64::from(record.raw_size),
        })? as usize;
        let rest = block.get(1 + name_len..).ok_or_else(|| Error::Truncated {
            name: record.path.clone(),
            offset: record.offset,
            size: u64::from(record.raw_size),
        })?;
        if !record.compressed {
            return Ok(rest.to_vec());
        }
        if rest.len() < 4 {
            return Err(Error::Truncated {
                name: record.path.clone(),
                offset: record.offset,
                size: u64::from(record.raw_size),
            });
        }
        let unc = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]);
        return if is_sse(format) {
            decompress_lz4_frame(&rest[4..], unc, &record.path)
        } else {
            decompress_zlib(&rest[4..], unc, &record.path)
        };
    }
    if record.compressed && record.raw_size > 0 {
        let compressed = slice(
            data,
            &record.path,
            record.offset,
            u64::from(record.raw_size),
        )?;
        // Skyrim SE archives pack with LZ4 frames, everything older with zlib.
        return if is_sse(format) {
            decompress_lz4_frame(compressed, record.uncompressed_size, &record.path)
        } else {
            decompress_zlib(compressed, record.uncompressed_size, &record.path)
        };
    }
    let size = if record.uncompressed_size > 0 {
        record.uncompressed_size
    } else {
        record.raw_size
    };
    slice(data, &record.path, record.offset, u64::from(size)).map(<[u8]>::to_vec)
}

/// Extract every file into `output_dir`, returning how many.
pub fn extract_archive(archive_path: &Path, output_dir: &Path) -> Result<u32> {
    let archive = Archive::open(archive_path)?;
    let mut count = 0;
    for i in 0..archive.index.records.len() {
        write_out(
            output_dir,
            &archive.index.records[i].path,
            &archive.read_record(i)?,
        )?;
        count += 1;
    }
    Ok(count)
}

/// Extract one file by its archive path into `output_dir`.
pub fn extract_file(archive_path: &Path, file_name: &str, output_dir: &Path) -> Result<()> {
    let archive = Archive::open(archive_path)?;
    let i = archive.find(file_name).ok_or_else(|| Error::NotFound {
        name: file_name.to_string(),
    })?;
    write_out(
        output_dir,
        &archive.index.records[i].path,
        &archive.read_record(i)?,
    )
}

fn write_out(output_dir: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    let out_path = output_dir.join(rel);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out_path, bytes)?;
    Ok(())
}

/// The record's declared unpacked length against what came out.
///
/// Checked only when the archive states a length. An off-disk index leaves it
/// zero, since the value lives out in the payload and only a read needs it,
/// and a genuinely empty file is not a mismatch to report either.
fn check_size(codec: &'static str, name: &str, want: u32, got: usize) -> Result<()> {
    if want > 0 && got != want as usize {
        return Err(Error::DecompressedSize {
            codec,
            name: name.to_string(),
            want,
            got,
        });
    }
    Ok(())
}

fn decompress_lz4_frame(data: &[u8], expected_size: u32, name: &str) -> Result<Vec<u8>> {
    let mut decoder = lz4_flex::frame::FrameDecoder::new(data);
    let mut output = Vec::with_capacity(expected_size as usize);
    decoder
        .read_to_end(&mut output)
        .map_err(|source| Error::Decompress {
            codec: "lz4",
            name: name.to_string(),
            source,
        })?;
    check_size("lz4", name, expected_size, output.len())?;
    Ok(output)
}

fn decompress_zlib(data: &[u8], expected_size: u32, name: &str) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::ZlibDecoder::new(data);
    let mut output = Vec::with_capacity(expected_size as usize);
    decoder
        .read_to_end(&mut output)
        .map_err(|source| Error::Decompress {
            codec: "zlib",
            name: name.to_string(),
            source,
        })?;
    check_size("zlib", name, expected_size, output.len())?;
    Ok(output)
}

/// A DDS file from BA2 DX10 chunks: the header the archive dropped, then each
/// chunk's pixels in order.
fn rebuild_dds(data: &[u8], name: &str, dds: &DdsInfo) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_dds_header(&mut out, dds)?;
    for chunk in &dds.chunks {
        if chunk.packed_size > 0 {
            let compressed = slice(data, name, chunk.offset, u64::from(chunk.packed_size))?;
            out.extend(decompress_zlib(compressed, chunk.unpacked_size, name)?);
        } else {
            out.extend_from_slice(slice(
                data,
                name,
                chunk.offset,
                u64::from(chunk.unpacked_size),
            )?);
        }
    }
    Ok(out)
}

fn rebuild_dds_from_disk(
    file: &Mutex<std::fs::File>,
    len: u64,
    name: &str,
    dds: &DdsInfo,
    archive: &Path,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_dds_header(&mut out, dds)?;
    for chunk in &dds.chunks {
        if chunk.packed_size > 0 {
            let compressed = read_disk_range(
                file,
                len,
                name,
                chunk.offset,
                u64::from(chunk.packed_size),
                archive,
            )?;
            out.extend(decompress_zlib(&compressed, chunk.unpacked_size, name)?);
        } else {
            out.extend(read_disk_range(
                file,
                len,
                name,
                chunk.offset,
                u64::from(chunk.unpacked_size),
                archive,
            )?);
        }
    }
    Ok(out)
}

fn write_dds_header<W: Write>(w: &mut W, dds: &DdsInfo) -> Result<()> {
    w.write_all(b"DDS ")?;
    // DDS_HEADER, 124 bytes
    w.write_u32::<LittleEndian>(124)?;
    // CAPS | HEIGHT | WIDTH | PIXELFORMAT | MIPMAPCOUNT
    w.write_u32::<LittleEndian>(0x1 | 0x2 | 0x4 | 0x1000 | 0x20000)?;
    w.write_u32::<LittleEndian>(u32::from(dds.height))?;
    w.write_u32::<LittleEndian>(u32::from(dds.width))?;
    w.write_u32::<LittleEndian>(0)?; // pitch or linear size
    w.write_u32::<LittleEndian>(0)?; // depth
    w.write_u32::<LittleEndian>(u32::from(dds.num_mips))?;
    for _ in 0..11 {
        w.write_u32::<LittleEndian>(0)?; // reserved
    }
    // DDS_PIXELFORMAT, 32 bytes, FOURCC DX10 so the format lives in the extension
    w.write_u32::<LittleEndian>(32)?;
    w.write_u32::<LittleEndian>(0x4)?;
    w.write_all(b"DX10")?;
    for _ in 0..5 {
        w.write_u32::<LittleEndian>(0)?;
    }
    w.write_u32::<LittleEndian>(0x1000)?; // DDSCAPS_TEXTURE
    for _ in 0..4 {
        w.write_u32::<LittleEndian>(0)?;
    }
    // DDS_HEADER_DXT10, 20 bytes
    w.write_u32::<LittleEndian>(u32::from(dds.dxgi_format))?;
    w.write_u32::<LittleEndian>(3)?; // TEXTURE2D
    w.write_u32::<LittleEndian>(0)?;
    w.write_u32::<LittleEndian>(1)?; // array size
    w.write_u32::<LittleEndian>(0)?;
    Ok(())
}

#[must_use]
pub fn is_bsa_or_ba2(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
    matches!(ext.as_deref(), Some("bsa" | "ba2"))
}

/// Bethesda convention: `MyMod.esp` auto-loads `MyMod.bsa`, `MyMod - Textures.bsa`
/// and any other archive whose name starts with the plugin's stem.
pub fn find_associated_archives(plugin_name: &str, data_dir: &Path) -> Result<Vec<PathBuf>> {
    let stem = Path::new(plugin_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mut archives = Vec::new();
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            if !is_bsa_or_ba2(&entry.path()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.starts_with(&stem) {
                archives.push(entry.path());
            }
        }
    }
    archives.sort();
    Ok(archives)
}

/// Every archive in a directory, indexed off disk, with the ones that failed
/// kept apart so one bad download does not hide the rest.
#[derive(Debug, Default)]
pub struct Scan {
    pub archives: Vec<(PathBuf, ArchiveIndex)>,
    pub failed: Vec<(PathBuf, Error)>,
}

pub fn scan_archives(data_dir: &Path) -> Result<Scan> {
    let mut scan = Scan::default();
    if !data_dir.exists() {
        return Ok(scan);
    }
    for entry in std::fs::read_dir(data_dir)?.flatten() {
        let path = entry.path();
        if is_bsa_or_ba2(&path) {
            match index_archive(&path) {
                Ok(index) => scan.archives.push((path, index)),
                Err(e) => scan.failed.push((path, e)),
            }
        }
    }
    scan.archives.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(scan)
}

/// Skyrim SE stores names in Windows-1252, everything older in what is in
/// practice ASCII. Decoding 1252 for every BSA version is lossless for ASCII
/// and right for the accented names Skyrim SE mods ship.
fn decode_name(bytes: &[u8]) -> String {
    let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
    s.replace('\\', "/")
}

fn parse_bsa<R: Read + Seek>(
    cursor: &mut R,
    len: u64,
    path: &Path,
    peek_payload: bool,
) -> Result<ArchiveIndex> {
    cursor.seek(SeekFrom::Start(4))?;

    let version = cursor.read_u32::<LittleEndian>()?;
    let bsa_version = match version {
        103 => BsaVersion::Oblivion,
        104 => BsaVersion::Fo3Skyrim,
        105 => BsaVersion::SkyrimSe,
        _ => {
            return Err(Error::format(
                format!("Unknown BSA version {version}"),
                path,
            ))
        }
    };

    let _folder_record_offset = cursor.read_u32::<LittleEndian>()?; // always 36
    let archive_flags = cursor.read_u32::<LittleEndian>()?;
    let folder_count = cursor.read_u32::<LittleEndian>()?;
    let file_count = cursor.read_u32::<LittleEndian>()?;
    let _total_folder_name_len = cursor.read_u32::<LittleEndian>()?;
    let _total_file_name_len = cursor.read_u32::<LittleEndian>()?;
    let _file_flags = cursor.read_u32::<LittleEndian>()?;

    let has_dir_names = archive_flags & BSA_FLAG_INCLUDE_DIR_NAMES != 0;
    let has_file_names = archive_flags & BSA_FLAG_INCLUDE_FILE_NAMES != 0;
    let default_compressed = archive_flags & BSA_FLAG_COMPRESSED != 0;
    // Skyrim SE texture archives set this: every file block carries its own
    // path before the data.
    let embed_names = archive_flags & BSA_FLAG_EMBED_FILE_NAMES != 0;

    if !has_file_names {
        return Ok(ArchiveIndex {
            format: ArchiveFormat::Bsa(bsa_version),
            file_count,
            files: Vec::new(),
            records: Vec::new(),
        });
    }

    // A count the file cannot hold is a corrupt header, not a request for
    // gigabytes of Vec.
    let folder_record_size = if version >= 105 { 24 } else { 16 };
    if u64::from(folder_count) * folder_record_size > len || u64::from(file_count) * 16 > len {
        return Err(Error::format(
            format!(
                "Header claims {folder_count} folders and {file_count} files, more than the archive can hold"
            ),
            path,
        ));
    }

    let mut folder_file_counts = Vec::with_capacity(folder_count as usize);
    let mut stored_folder_hashes = Vec::with_capacity(folder_count as usize);
    for _ in 0..folder_count {
        let hash = cursor.read_u64::<LittleEndian>()?;
        stored_folder_hashes.push(hash);
        let count = cursor.read_u32::<LittleEndian>()?;
        if version >= 105 {
            let _padding = cursor.read_u32::<LittleEndian>()?;
            let _offset = cursor.read_u64::<LittleEndian>()?;
        } else {
            let _offset = cursor.read_u32::<LittleEndian>()?;
        }
        folder_file_counts.push(count);
    }

    let mut folder_names: Vec<String> = Vec::with_capacity(folder_count as usize);
    let mut raw_file_records: Vec<(u64, u32, u32)> = Vec::with_capacity(file_count as usize);

    for &count in &folder_file_counts {
        let folder_name = if has_dir_names {
            let name_len = cursor.read_u8()? as usize;
            let mut name_bytes = vec![0u8; name_len];
            cursor.read_exact(&mut name_bytes)?;
            if name_bytes.last() == Some(&0) {
                name_bytes.pop();
            }
            decode_name(&name_bytes)
        } else {
            String::new()
        };
        folder_names.push(folder_name);

        if has_dir_names {
            let expected = folder_hash(&folder_names[folder_names.len() - 1]);
            let actual = stored_folder_hashes[folder_names.len() - 1];
            if actual != expected {
                return Err(Error::format(
                    format!("BSA directory hash mismatch for '{}': stored {actual:#018x}, expected {expected:#018x}", folder_names[folder_names.len() - 1]),
                    path,
                ));
            }
        }

        for _ in 0..count {
            let hash = cursor.read_u64::<LittleEndian>()?;
            let size = cursor.read_u32::<LittleEndian>()?;
            let offset = cursor.read_u32::<LittleEndian>()?;
            raw_file_records.push((hash, size, offset));
        }
    }

    let mut files = Vec::with_capacity(file_count as usize);
    let mut records = Vec::with_capacity(file_count as usize);
    let mut folder_idx = 0;
    let mut file_in_folder = 0;

    for &(stored_hash, raw_size_field, raw_offset) in
        raw_file_records.iter().take(file_count as usize)
    {
        let file_name = read_null_terminated(cursor, len)?;
        let expected_hash = file_name_hash(&file_name);
        let folder_name = folder_names.get(folder_idx).map_or("", String::as_str);
        let full_path = if folder_name.is_empty() {
            file_name
        } else {
            format!("{folder_name}/{file_name}")
        };
        if stored_hash != expected_hash {
            return Err(Error::format(
                format!("BSA file hash mismatch for '{full_path}': stored {stored_hash:#018x}, expected {expected_hash:#018x}"),
                path,
            ));
        }
        let compression_toggled = raw_size_field & BSA_SIZE_COMPRESSION_BIT != 0;
        let data_size = raw_size_field & !BSA_SIZE_COMPRESSION_BIT;
        let is_compressed = default_compressed ^ compression_toggled;

        let record = if embed_names {
            // The block is `u8 len, name, [u32 uncompressed size], payload`.
            // Its offset and full size are kept as stored and the read peels
            // the prefix, since the name length is out in the payload too.
            let ofs = u64::from(raw_offset);
            let unc_size = if peek_payload && is_compressed && ofs < len {
                let here = cursor.stream_position()?;
                cursor.seek(SeekFrom::Start(ofs))?;
                let name_len = u64::from(cursor.read_u8()?);
                cursor.seek(SeekFrom::Start(ofs + 1 + name_len))?;
                let v = cursor.read_u32::<LittleEndian>()?;
                cursor.seek(SeekFrom::Start(here))?;
                v
            } else {
                0
            };
            FileRecord {
                path: full_path.clone(),
                offset: ofs,
                raw_size: data_size,
                compressed: is_compressed,
                uncompressed_size: unc_size,
                name_embedded: true,
                dds_info: None,
            }
        } else {
            // A compressed file starts with its uncompressed size as a u32,
            // then the packed stream. That u32 is the one thing out in the
            // payload, so an off-disk index takes the offsets and leaves the
            // size at 0.
            let (actual_offset, compressed_size, uncompressed_size) =
                if is_compressed && data_size > 4 {
                    let ofs = u64::from(raw_offset);
                    if ofs + 4 <= len {
                        let unc_size = if peek_payload {
                            let here = cursor.stream_position()?;
                            cursor.seek(SeekFrom::Start(ofs))?;
                            let v = cursor.read_u32::<LittleEndian>()?;
                            cursor.seek(SeekFrom::Start(here))?;
                            v
                        } else {
                            0
                        };
                        (ofs + 4, data_size - 4, unc_size)
                    } else {
                        (ofs, data_size, 0)
                    }
                } else {
                    (u64::from(raw_offset), data_size, data_size)
                };
            FileRecord {
                path: full_path.clone(),
                offset: actual_offset,
                raw_size: compressed_size,
                compressed: is_compressed && data_size > 4,
                uncompressed_size,
                name_embedded: false,
                dds_info: None,
            }
        };
        records.push(record);
        files.push(full_path);

        file_in_folder += 1;
        if folder_idx < folder_file_counts.len()
            && file_in_folder >= folder_file_counts[folder_idx] as usize
        {
            folder_idx += 1;
            file_in_folder = 0;
        }
    }

    Ok(ArchiveIndex {
        format: ArchiveFormat::Bsa(bsa_version),
        file_count,
        files,
        records,
    })
}

fn parse_ba2<R: Read + Seek>(cursor: &mut R, len: u64, path: &Path) -> Result<ArchiveIndex> {
    cursor.seek(SeekFrom::Start(4))?;

    let version = cursor.read_u32::<LittleEndian>()?;
    if version != 1 {
        return Err(Error::format(
            format!("Unknown BA2 version {version}"),
            path,
        ));
    }

    let mut type_bytes = [0u8; 4];
    cursor.read_exact(&mut type_bytes)?;
    let ba2_type = match &type_bytes {
        b"GNRL" => Ba2Type::General,
        b"DX10" => Ba2Type::Dx10,
        _ => {
            return Err(Error::format(
                format!(
                    "Unknown BA2 type {:?}",
                    String::from_utf8_lossy(&type_bytes)
                ),
                path,
            ))
        }
    };

    let file_count = cursor.read_u32::<LittleEndian>()?;
    let name_table_offset = cursor.read_u64::<LittleEndian>()?;
    if u64::from(file_count) * 24 > len {
        return Err(Error::format(
            format!("Header claims {file_count} files, more than the archive can hold"),
            path,
        ));
    }

    let mut records = Vec::with_capacity(file_count as usize);
    match ba2_type {
        Ba2Type::General => {
            for _ in 0..file_count {
                let _name_hash = cursor.read_u32::<LittleEndian>()?;
                let mut _ext = [0u8; 4];
                cursor.read_exact(&mut _ext)?;
                let _dir_hash = cursor.read_u32::<LittleEndian>()?;
                let _unknown = cursor.read_u32::<LittleEndian>()?;
                let offset = cursor.read_u64::<LittleEndian>()?;
                let packed_size = cursor.read_u32::<LittleEndian>()?;
                let unpacked_size = cursor.read_u32::<LittleEndian>()?;
                let _unknown2 = cursor.read_u32::<LittleEndian>()?;
                records.push(FileRecord {
                    path: String::new(),
                    offset,
                    raw_size: if packed_size > 0 {
                        packed_size
                    } else {
                        unpacked_size
                    },
                    compressed: packed_size > 0,
                    uncompressed_size: unpacked_size,
                    name_embedded: false,
                    dds_info: None,
                });
            }
        }
        Ba2Type::Dx10 => {
            for _ in 0..file_count {
                let _name_hash = cursor.read_u32::<LittleEndian>()?;
                let mut _ext = [0u8; 4];
                cursor.read_exact(&mut _ext)?;
                let _dir_hash = cursor.read_u32::<LittleEndian>()?;
                let _unknown = cursor.read_u8()?;
                let num_chunks = cursor.read_u8()?;
                let chunk_header_size = cursor.read_u16::<LittleEndian>()?;
                if chunk_header_size != 24 {
                    return Err(Error::format(
                        format!("Invalid BA2 DX10 chunk header size {chunk_header_size}"),
                        path,
                    ));
                }
                let height = cursor.read_u16::<LittleEndian>()?;
                let width = cursor.read_u16::<LittleEndian>()?;
                let num_mips = cursor.read_u8()?;
                let dxgi_format = cursor.read_u8()?;
                let _tile_mode = cursor.read_u16::<LittleEndian>()?;

                let mut chunks = Vec::with_capacity(num_chunks as usize);
                for _ in 0..num_chunks {
                    let chunk_offset = cursor.read_u64::<LittleEndian>()?;
                    let packed_size = cursor.read_u32::<LittleEndian>()?;
                    let unpacked_size = cursor.read_u32::<LittleEndian>()?;
                    let _start_mip = cursor.read_u16::<LittleEndian>()?;
                    let _end_mip = cursor.read_u16::<LittleEndian>()?;
                    let sentinel = cursor.read_u32::<LittleEndian>()?;
                    if sentinel != BA2_DX10_CHUNK_SENTINEL {
                        return Err(Error::format(
                            format!("Invalid BA2 DX10 chunk sentinel {sentinel:#010x}"),
                            path,
                        ));
                    }
                    chunks.push(DdsChunk {
                        offset: chunk_offset,
                        packed_size,
                        unpacked_size,
                    });
                }
                records.push(FileRecord {
                    path: String::new(),
                    offset: 0,
                    raw_size: 0,
                    compressed: false,
                    uncompressed_size: 0,
                    name_embedded: false,
                    dds_info: Some(DdsInfo {
                        height,
                        width,
                        num_mips,
                        dxgi_format,
                        chunks,
                    }),
                });
            }
        }
    }

    let mut files = Vec::with_capacity(file_count as usize);
    if name_table_offset > 0 && name_table_offset < len {
        cursor.seek(SeekFrom::Start(name_table_offset))?;
        for i in 0..file_count as usize {
            if cursor.stream_position()? >= len {
                break;
            }
            let name_len = cursor.read_u16::<LittleEndian>()? as usize;
            let mut name_bytes = vec![0u8; name_len];
            cursor.read_exact(&mut name_bytes)?;
            let name = decode_name(&name_bytes);
            if i < records.len() {
                records[i].path = name.clone();
            }
            files.push(name);
        }
    }

    Ok(ArchiveIndex {
        format: ArchiveFormat::Ba2(ba2_type),
        file_count,
        files,
        records,
    })
}

fn read_null_terminated<R: Read + Seek>(cursor: &mut R, len: u64) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        if cursor.stream_position()? >= len {
            break;
        }
        let b = cursor.read_u8()?;
        if b == 0 {
            break;
        }
        bytes.push(b);
    }
    Ok(decode_name(&bytes))
}

#[cfg(test)]
pub(crate) mod testdata {
    use std::io::Write;

    use crate::hash::{file_name_hash, folder_hash};

    /// A v104 BSA with one zlib-compressed file at `meshes/a.nif`.
    pub fn compressed_bsa(payload: &[u8]) -> (Vec<u8>, u32) {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(payload).unwrap();
        let zlib = enc.finish().unwrap();

        let folder = b"meshes\0";
        let name = b"a.nif\0";
        // 36 header + 16 folder record + (1 + 7 + 16) file block + 6 names
        let data_offset: u32 = 36 + 16 + (1 + folder.len() as u32 + 16) + name.len() as u32;
        let data_size: u32 = 4 + zlib.len() as u32;

        let mut d = Vec::new();
        d.extend_from_slice(b"BSA\0");
        d.extend_from_slice(&104u32.to_le_bytes());
        d.extend_from_slice(&36u32.to_le_bytes());
        d.extend_from_slice(&0x07u32.to_le_bytes()); // dir names + file names + compressed
        d.extend_from_slice(&1u32.to_le_bytes()); // folders
        d.extend_from_slice(&1u32.to_le_bytes()); // files
        d.extend_from_slice(&(folder.len() as u32).to_le_bytes());
        d.extend_from_slice(&(name.len() as u32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // file flags
        d.extend_from_slice(&folder_hash("meshes").to_le_bytes()); // folder hash
        d.extend_from_slice(&1u32.to_le_bytes()); // file count in folder
        d.extend_from_slice(&0u32.to_le_bytes()); // folder offset (unused here)
        d.push(folder.len() as u8);
        d.extend_from_slice(folder);
        d.extend_from_slice(&file_name_hash("a.nif").to_le_bytes()); // file hash
        d.extend_from_slice(&data_size.to_le_bytes());
        d.extend_from_slice(&data_offset.to_le_bytes());
        d.extend_from_slice(name);
        assert_eq!(d.len() as u32, data_offset);
        d.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        d.extend_from_slice(&zlib);
        (d, payload.len() as u32)
    }

    /// A v105 BSA with uncompressed files, each `(folder, name, bytes)`. Names
    /// are written as Windows-1252 so a test can put an accented name in.
    /// Files of one folder must be listed together.
    pub fn sse_bsa(files: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let mut folders: Vec<&str> = Vec::new();
        for (f, _, _) in files {
            if !folders.contains(f) {
                folders.push(f);
            }
        }
        let enc = |s: &str| -> Vec<u8> {
            s.chars()
                .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
                .collect()
        };
        // One byte per char once encoded, so count chars, not UTF-8 bytes.
        let flen = |s: &str| s.chars().count() as u32;
        let folder_names_len: u32 = folders.iter().map(|f| flen(f) + 1).sum();
        let file_names_len: u32 = files.iter().map(|(_, n, _)| flen(n) + 1).sum();
        let header = 36u32;
        let folder_records = folders.len() as u32 * 24;
        let file_blocks: u32 = folders
            .iter()
            .map(|f| 1 + flen(f) + 1 + files.iter().filter(|(g, _, _)| g == f).count() as u32 * 16)
            .sum();
        let data_start = header + folder_records + file_blocks + file_names_len;

        let mut d = Vec::new();
        d.extend_from_slice(b"BSA\0");
        d.extend_from_slice(&105u32.to_le_bytes());
        d.extend_from_slice(&36u32.to_le_bytes());
        d.extend_from_slice(&0x03u32.to_le_bytes()); // names, no compression
        d.extend_from_slice(&(folders.len() as u32).to_le_bytes());
        d.extend_from_slice(&(files.len() as u32).to_le_bytes());
        d.extend_from_slice(&folder_names_len.to_le_bytes());
        d.extend_from_slice(&file_names_len.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        for f in &folders {
            let count = files.iter().filter(|(g, _, _)| g == f).count() as u32;
            d.extend_from_slice(&folder_hash(f).to_le_bytes());
            d.extend_from_slice(&count.to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&0u64.to_le_bytes());
        }
        let mut offset = data_start;
        for f in &folders {
            let fb = enc(f);
            d.push(fb.len() as u8 + 1);
            d.extend_from_slice(&fb);
            d.push(0);
            for (_, name, bytes) in files.iter().filter(|(g, _, _)| g == f) {
                d.extend_from_slice(&file_name_hash(name).to_le_bytes());
                d.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                d.extend_from_slice(&offset.to_le_bytes());
                offset += bytes.len() as u32;
            }
        }
        for (_, n, _) in files {
            d.extend_from_slice(&enc(n));
            d.push(0);
        }
        assert_eq!(d.len() as u32, data_start);
        for (_, _, bytes) in files {
            d.extend_from_slice(bytes);
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::testdata::{compressed_bsa, sse_bsa};
    use super::*;

    #[test]
    fn is_bsa_detects_extensions() {
        assert!(is_bsa_or_ba2(Path::new("Skyrim - Meshes0.bsa")));
        assert!(is_bsa_or_ba2(Path::new("Fallout4 - Textures.ba2")));
        assert!(is_bsa_or_ba2(Path::new("test.BSA")));
        assert!(is_bsa_or_ba2(Path::new("test.BA2")));
        assert!(!is_bsa_or_ba2(Path::new("test.esp")));
        assert!(!is_bsa_or_ba2(Path::new("test.esm")));
        assert!(!is_bsa_or_ba2(Path::new("readme.txt")));
    }

    #[test]
    fn archive_format_display() {
        assert_eq!(
            format!("{}", ArchiveFormat::Bsa(BsaVersion::SkyrimSe)),
            "BSA (v105/SkyrimSE)"
        );
        assert_eq!(
            format!("{}", ArchiveFormat::Ba2(Ba2Type::General)),
            "BA2 (General)"
        );
    }

    #[test]
    fn fmt_14_accepts_hashes_from_the_reference_vectors() {
        let folder = "textures/armor/amuletsandrings/elder council";
        let name = "elder_council_amulet_n.dds";
        assert_eq!(folder_hash(folder), 0x04BC_422C_742C_696C);
        assert_eq!(file_name_hash(name), 0xDC53_1E2F_6516_DFEE);

        let data = sse_bsa(&[(folder, name, b"fixture")]);
        let index = index_archive_in(&data, Path::new("reference-hashes.bsa")).unwrap();
        assert_eq!(index.files, [format!("{folder}/{name}")]);
    }

    #[test]
    fn fmt_14_rejects_a_wrong_file_path_hash() {
        let data = sse_bsa(&[("meshes", "a.nif", b"fixture")]);
        let file_hash_at = 36 + 24 + 1 + "meshes".len() + 1;
        let mut wrong_hash = [0u8; 8];
        wrong_hash.copy_from_slice(&data[file_hash_at..file_hash_at + 8]);
        wrong_hash[0] ^= 1;
        let mut data = data;
        data[file_hash_at..file_hash_at + 8].copy_from_slice(&wrong_hash);

        let error = index_archive_in(&data, Path::new("wrong-file-hash.bsa")).unwrap_err();
        assert!(
            error.to_string().contains("BSA file hash mismatch"),
            "{error}"
        );
        assert!(error.to_string().contains("meshes/a.nif"), "{error}");
    }

    #[test]
    fn fmt_14_rejects_a_wrong_directory_path_hash() {
        let mut data = sse_bsa(&[("meshes", "a.nif", b"fixture")]);
        data[36] ^= 1;

        let error = index_archive_in(&data, Path::new("wrong-directory-hash.bsa")).unwrap_err();
        assert!(
            error.to_string().contains("BSA directory hash mismatch"),
            "{error}"
        );
        assert!(error.to_string().contains("meshes"), "{error}");
    }

    #[test]
    fn too_small_file_errors() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("tiny.bsa");
        std::fs::write(&path, [0u8; 4]).unwrap();
        assert!(index_archive(&path).is_err());
    }

    #[test]
    fn unknown_magic_errors() {
        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("bad.bsa");
        std::fs::write(&path, b"NOPE0000extra_padding_bytes").unwrap();
        let err = index_archive(&path).unwrap_err();
        assert!(err.to_string().contains("Unknown archive format"));
    }

    #[test]
    fn parse_minimal_bsa_header() {
        let mut data = Vec::new();
        data.extend_from_slice(b"BSA\0");
        data.extend_from_slice(&105u32.to_le_bytes());
        data.extend_from_slice(&36u32.to_le_bytes());
        data.extend_from_slice(&0x03u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("empty.bsa");
        std::fs::write(&path, &data).unwrap();

        let index = index_archive(&path).unwrap();
        assert_eq!(index.format, ArchiveFormat::Bsa(BsaVersion::SkyrimSe));
        assert_eq!(index.file_count, 0);
        assert!(index.files.is_empty());
        assert!(index.records.is_empty());
    }

    #[test]
    fn parse_minimal_ba2_header() {
        let mut data = Vec::new();
        data.extend_from_slice(b"BTDX");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(b"GNRL");
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u64.to_le_bytes());

        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("empty.ba2");
        std::fs::write(&path, &data).unwrap();

        let index = index_archive(&path).unwrap();
        assert_eq!(index.format, ArchiveFormat::Ba2(Ba2Type::General));
        assert_eq!(index.file_count, 0);
        assert!(index.records.is_empty());
    }

    fn gnrl_record(data: &mut Vec<u8>, offset: u64, packed: u32, unpacked: u32) {
        data.extend_from_slice(&0u32.to_le_bytes()); // name hash
        data.extend_from_slice(&[0u8; 4]); // ext
        data.extend_from_slice(&0u32.to_le_bytes()); // dir hash
        data.extend_from_slice(&0u32.to_le_bytes()); // unknown
        data.extend_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(&packed.to_le_bytes());
        data.extend_from_slice(&unpacked.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // unknown2
    }

    #[test]
    fn ba2_with_name_table() {
        let mut data = Vec::new();
        data.extend_from_slice(b"BTDX");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(b"GNRL");
        data.extend_from_slice(&2u32.to_le_bytes());
        // Header is 24 bytes, each GNRL record is 36 bytes.
        let name_table_offset = 24u64 + 72;
        data.extend_from_slice(&name_table_offset.to_le_bytes());
        gnrl_record(&mut data, 200, 0, 100);
        gnrl_record(&mut data, 300, 0, 50);
        for name in [&b"meshes\\test.nif"[..], &b"textures\\test.dds"[..]] {
            data.extend_from_slice(&(name.len() as u16).to_le_bytes());
            data.extend_from_slice(name);
        }

        let tmpdir = tempfile::tempdir().unwrap();
        let path = tmpdir.path().join("test.ba2");
        std::fs::write(&path, &data).unwrap();

        let index = index_archive(&path).unwrap();
        assert_eq!(index.file_count, 2);
        assert_eq!(index.files, vec!["meshes/test.nif", "textures/test.dds"]);
        assert_eq!(index.records[0].offset, 200);
        assert_eq!(index.records[0].uncompressed_size, 100);
        assert!(!index.records[0].compressed);
        assert_eq!(index.records[1].offset, 300);
        assert_eq!(index.records[1].uncompressed_size, 50);
    }

    /// A GNRL BA2 with the given `(name, stored bytes, unpacked size)` files,
    /// `packed` true when the stored bytes are zlib.
    fn gnrl_ba2(files: &[(&[u8], &[u8], u32)], packed: bool) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"BTDX");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(b"GNRL");
        data.extend_from_slice(&(files.len() as u32).to_le_bytes());
        let record_end = 24u64 + 36 * files.len() as u64;
        let name_table_size: u64 = files.iter().map(|(n, _, _)| 2 + n.len() as u64).sum();
        data.extend_from_slice(&record_end.to_le_bytes());
        let mut offset = record_end + name_table_size;
        for (_, bytes, unpacked) in files {
            let packed_size = if packed { bytes.len() as u32 } else { 0 };
            gnrl_record(&mut data, offset, packed_size, *unpacked);
            offset += bytes.len() as u64;
        }
        for (n, _, _) in files {
            data.extend_from_slice(&(n.len() as u16).to_le_bytes());
            data.extend_from_slice(n);
        }
        for (_, bytes, _) in files {
            data.extend_from_slice(bytes);
        }
        data
    }

    #[test]
    fn extract_ba2_gnrl_uncompressed() {
        let file_data = b"Hello from inside a BA2!";
        let data = gnrl_ba2(
            &[(b"test/hello.txt", file_data, file_data.len() as u32)],
            false,
        );

        let tmpdir = tempfile::tempdir().unwrap();
        let archive_path = tmpdir.path().join("test.ba2");
        std::fs::write(&archive_path, &data).unwrap();
        let out_dir = tmpdir.path().join("output");
        std::fs::create_dir(&out_dir).unwrap();

        let count = extract_archive(&archive_path, &out_dir).unwrap();
        assert_eq!(count, 1);
        let extracted = std::fs::read(out_dir.join("test/hello.txt")).unwrap();
        assert_eq!(extracted, file_data);
    }

    #[test]
    fn extract_ba2_gnrl_compressed() {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        let original = b"This is some test data that will be compressed with zlib";
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();
        let data = gnrl_ba2(
            &[(b"data/compressed.txt", &compressed, original.len() as u32)],
            true,
        );

        let tmpdir = tempfile::tempdir().unwrap();
        let archive_path = tmpdir.path().join("compressed.ba2");
        std::fs::write(&archive_path, &data).unwrap();
        let out_dir = tmpdir.path().join("output");
        std::fs::create_dir(&out_dir).unwrap();

        let memory = Archive::from_bytes(&archive_path, data.clone()).unwrap();
        let disk = Archive::open(&archive_path).unwrap();
        assert_eq!(memory.read("data/compressed.txt").unwrap(), original);
        assert_eq!(disk.read("data/compressed.txt").unwrap(), original);
        assert_eq!(extract_archive(&archive_path, &out_dir).unwrap(), 1);
        let extracted = std::fs::read(out_dir.join("data/compressed.txt")).unwrap();
        assert_eq!(extracted, original);
    }

    #[test]
    fn extract_single_file_by_name() {
        let file1_data = b"first file";
        let file2_data = b"second file";
        let data = gnrl_ba2(
            &[
                (b"meshes/a.nif", file1_data, file1_data.len() as u32),
                (b"meshes/b.nif", file2_data, file2_data.len() as u32),
            ],
            false,
        );

        let tmpdir = tempfile::tempdir().unwrap();
        let archive_path = tmpdir.path().join("multi.ba2");
        std::fs::write(&archive_path, &data).unwrap();
        let out_dir = tmpdir.path().join("output");
        std::fs::create_dir(&out_dir).unwrap();

        extract_file(&archive_path, "meshes/b.nif", &out_dir).unwrap();
        assert!(!out_dir.join("meshes/a.nif").exists());
        assert_eq!(
            std::fs::read(out_dir.join("meshes/b.nif")).unwrap(),
            file2_data
        );

        // The in-memory API answers the same bytes, case and slash insensitive.
        let archive = Archive::open(&archive_path).unwrap();
        assert_eq!(archive.read("MESHES\\A.NIF").unwrap(), file1_data);
        assert!(archive.contains("meshes/b.nif"));
        assert!(!archive.contains("meshes/c.nif"));
    }

    #[test]
    fn extract_file_not_found() {
        let data = gnrl_ba2(&[], false);
        let tmpdir = tempfile::tempdir().unwrap();
        let archive_path = tmpdir.path().join("empty.ba2");
        std::fs::write(&archive_path, &data).unwrap();
        let out_dir = tmpdir.path().join("output");
        std::fs::create_dir(&out_dir).unwrap();

        let err = extract_file(&archive_path, "nonexistent.txt", &out_dir).unwrap_err();
        assert!(err.to_string().contains("not found in archive"));
    }

    #[test]
    fn find_associated_archives_matches_stem() {
        let tmpdir = tempfile::tempdir().unwrap();
        std::fs::write(tmpdir.path().join("MyMod.bsa"), b"BSA\0").unwrap();
        std::fs::write(tmpdir.path().join("MyMod - Textures.bsa"), b"BSA\0").unwrap();
        std::fs::write(tmpdir.path().join("OtherMod.bsa"), b"BSA\0").unwrap();
        std::fs::write(tmpdir.path().join("MyMod.esp"), b"TES4").unwrap();

        let archives = find_associated_archives("MyMod.esp", tmpdir.path()).unwrap();
        let names: Vec<String> = archives
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["MyMod - Textures.bsa", "MyMod.bsa"]);
    }

    #[test]
    fn scan_empty_dir() {
        let tmpdir = tempfile::tempdir().unwrap();
        let scan = scan_archives(tmpdir.path()).unwrap();
        assert!(scan.archives.is_empty());
        assert!(scan.failed.is_empty());
    }

    #[test]
    fn scan_nonexistent_dir() {
        let scan = scan_archives(Path::new("/nonexistent/path")).unwrap();
        assert!(scan.archives.is_empty());
    }

    #[test]
    fn scan_keeps_the_broken_archive_apart() {
        let tmpdir = tempfile::tempdir().unwrap();
        let (good, _) = compressed_bsa(b"x");
        std::fs::write(tmpdir.path().join("good.bsa"), &good).unwrap();
        std::fs::write(tmpdir.path().join("bad.bsa"), b"BSA\0garbage-garbage").unwrap();
        let scan = scan_archives(tmpdir.path()).unwrap();
        assert_eq!(scan.archives.len(), 1);
        assert_eq!(scan.failed.len(), 1);
        assert!(scan.failed[0].0.ends_with("bad.bsa"));
    }

    // Indexing off disk must agree with indexing in memory on everything but
    // uncompressed_size, which lives out in the payload.
    #[test]
    fn indexing_from_disk_matches_indexing_in_memory() {
        let (bytes, unc) = compressed_bsa(b"hello world, compressed");
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.bsa");
        std::fs::write(&path, &bytes).unwrap();

        let disk = index_archive(&path).unwrap();
        let mem = index_archive_in(&bytes, &path).unwrap();

        assert_eq!(disk.files, mem.files);
        assert_eq!(disk.files, vec!["meshes/a.nif".to_string()]);
        assert_eq!(disk.file_count, mem.file_count);
        assert_eq!(disk.records[0].offset, mem.records[0].offset);
        assert_eq!(disk.records[0].raw_size, mem.records[0].raw_size);
        assert_eq!(disk.records[0].compressed, mem.records[0].compressed);
        assert!(disk.records[0].compressed);
        assert_eq!(mem.records[0].uncompressed_size, unc);
        assert_eq!(disk.records[0].uncompressed_size, 0);
    }

    // ----- fault injection on the read path -----
    //
    // The plan asks for failpoints here. There is nowhere useful to put one:
    // the only IO is a single `std::fs::read` in `Archive::open`, and every
    // other step works on a `Vec<u8>` already in memory, so there is no
    // partial read to interrupt. Corrupting the bytes reaches the same code
    // with no dependency added, and reaches it deterministically rather than
    // through a global failpoint registry. The save write path, the other half
    // the plan pairs this with, is in a crate another fork holds.

    /// A declared uncompressed size larger than the stream really holds.
    ///
    /// The four bytes at the record's offset say how long the file is once
    /// decompressed, and the index peeks them. Nothing checked them against
    /// what came out, so a corrupt archive handed every caller a short buffer
    /// and called it success. A mesh, a texture or a script read that way is
    /// silently truncated, which is worse than an error.
    #[test]
    fn a_decompressed_file_shorter_than_declared_is_refused() {
        let payload = b"hello world, compressed";
        let (mut bytes, unc) = compressed_bsa(payload);
        assert_eq!(unc as usize, payload.len());

        let path = Path::new("c.bsa");
        let index = index_archive_in(&bytes, path).unwrap();
        let at = index.records[0].offset as usize - 4;

        // Claim the file is far longer than it is, leaving the stream alone.
        let lie = (payload.len() as u32 + 4096).to_le_bytes();
        bytes[at..at + 4].copy_from_slice(&lie);

        let archive = Archive::from_bytes(path, bytes).unwrap();
        match archive.read("meshes/a.nif") {
            Err(Error::DecompressedSize {
                codec, want, got, ..
            }) => {
                assert_eq!(codec, "zlib");
                assert_eq!(want as usize, payload.len() + 4096);
                assert_eq!(got, payload.len());
            }
            Err(other) => panic!("expected a size mismatch, got {other:?}"),
            Ok(out) => panic!(
                "a file declaring {} bytes came back with {} and no error",
                payload.len() + 4096,
                out.len()
            ),
        }
    }

    /// The same check must not fire on a file that is simply empty, and must
    /// not fire on a healthy read.
    #[test]
    fn an_honest_declared_size_still_reads() {
        for payload in [b"".as_slice(), b"x".as_slice(), b"a longer payload here"] {
            let (bytes, _) = compressed_bsa(payload);
            let archive = Archive::from_bytes(Path::new("c.bsa"), bytes).unwrap();
            assert_eq!(archive.read("meshes/a.nif").unwrap(), payload);
        }
    }

    /// Every prefix of a valid archive. A truncated file must be reported, at
    /// index time or at read time, and must never panic.
    #[test]
    fn every_truncation_of_an_archive_is_reported_not_fatal() {
        let (full, _) = compressed_bsa(b"a payload long enough to span blocks");
        let path = Path::new("t.bsa");
        for cut in 0..full.len() {
            let part = full[..cut].to_vec();
            // Repro for a failure here:
            //   cargo test -p soe-bsa every_truncation_of_an_archive_is_reported_not_fatal
            let Ok(archive) = Archive::from_bytes(path, part) else {
                continue;
            };
            for i in 0..archive.index.records.len() {
                let _ = archive.read_record(i);
            }
        }
    }

    /// One flipped bit at every position in the header and tables. The archive
    /// may refuse to index, may index and fail to read, or may read something
    /// wrong, and any of those is fine. A panic is not.
    #[test]
    fn a_single_flipped_bit_anywhere_in_the_index_is_survivable() {
        let (full, _) = compressed_bsa(b"a payload long enough to span blocks");
        let path = Path::new("b.bsa");
        let header_and_tables = full.len().min(96);
        for at in 0..header_and_tables {
            for bit in 0..8u32 {
                let mut bytes = full.clone();
                bytes[at] ^= 1 << bit;
                // Repro:
                //   cargo test -p soe-bsa a_single_flipped_bit_anywhere_in_the_index_is_survivable
                let Ok(archive) = Archive::from_bytes(path, bytes) else {
                    continue;
                };
                for i in 0..archive.index.records.len() {
                    let _ = archive.read_record(i);
                }
            }
        }
    }

    #[test]
    fn extracting_a_compressed_bsa_file_still_round_trips() {
        let payload = b"hello world, compressed";
        let (bytes, _) = compressed_bsa(payload);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.bsa");
        std::fs::write(&path, &bytes).unwrap();
        let out = tmp.path().join("out");

        extract_file(&path, "meshes/a.nif", &out).unwrap();
        assert_eq!(std::fs::read(out.join("meshes/a.nif")).unwrap(), payload);
    }

    #[test]
    fn disk_bsa_reads_match_memory_under_concurrency() {
        let payload = b"one compressed payload shared by concurrent readers";
        let (bytes, _) = compressed_bsa(payload);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("concurrent.bsa");
        std::fs::write(&path, &bytes).unwrap();
        let memory = Archive::from_bytes(&path, bytes).unwrap();
        let disk = std::sync::Arc::new(Archive::open(&path).unwrap());
        assert_eq!(memory.read("meshes/a.nif").unwrap(), payload);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let disk = std::sync::Arc::clone(&disk);
                scope.spawn(move || {
                    for _ in 0..32 {
                        assert_eq!(disk.read("MESHES\\A.NIF").unwrap(), payload);
                    }
                });
            }
        });
    }

    #[test]
    fn disk_open_keeps_a_large_payload_off_heap() {
        let data = sse_bsa(&[("meshes", "a.nif", b"x")]);
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sparse.bsa");
        std::fs::write(&path, data).unwrap();
        let size = 64 * 1024 * 1024;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(size)
            .unwrap();
        let archive = Archive::open(&path).unwrap();
        assert_eq!(archive.index().records.len(), 1);
        assert!(matches!(&archive.storage, ArchiveStorage::Disk { len, .. } if *len == size));
    }

    #[test]
    fn disk_payload_truncation_is_typed_after_indexing() {
        let (bytes, _) = compressed_bsa(b"payload");
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cut-after-open.bsa");
        std::fs::write(&path, &bytes).unwrap();
        let archive = Archive::open(&path).unwrap();
        std::fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
        assert!(matches!(
            archive.read("meshes/a.nif"),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn sse_archive_reads_lz4_frames_and_1252_names() {
        // Build a v105 archive by hand with one LZ4-framed file whose folder
        // has an accented name, the case Windows-1252 exists for.
        let payload = b"lz4 framed payload for skyrim se";
        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        enc.write_all(payload).unwrap();
        let lz4 = enc.finish().unwrap();
        let mut stored = (payload.len() as u32).to_le_bytes().to_vec();
        stored.extend_from_slice(&lz4);

        let mut d = sse_bsa(&[("textures/caf\u{e9}", "Sign.dds", &stored)]);
        // Flip the compression toggle bit on the one file record so the
        // archive-level "not compressed" default is inverted for it.
        let size_field_at = 36 + 24 + 1 + "textures/caf\u{e9}".chars().count() + 1 + 8;
        let size = u32::from_le_bytes(d[size_field_at..size_field_at + 4].try_into().unwrap());
        d[size_field_at..size_field_at + 4]
            .copy_from_slice(&(size | BSA_SIZE_COMPRESSION_BIT).to_le_bytes());

        let archive = Archive::from_bytes(Path::new("sse.bsa"), d).unwrap();
        assert_eq!(archive.format(), ArchiveFormat::Bsa(BsaVersion::SkyrimSe));
        assert_eq!(archive.files(), ["textures/caf\u{e9}/Sign.dds"]);
        assert_eq!(
            archive.read("Textures/Caf\u{c9}/sign.dds").unwrap(),
            payload
        );
    }

    #[test]
    fn embedded_names_are_peeled_off_stored_and_compressed_blocks() {
        // The layout Skyrim SE texture archives use: flag 0x100, every block
        // `u8 len, path, [u32 size], payload`. One stored file, one LZ4.
        let plain = b"stored pixels";
        let packed_src = b"packed pixels, packed pixels, packed pixels";
        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        enc.write_all(packed_src).unwrap();
        let lz4 = enc.finish().unwrap();

        let folder = b"textures";
        let names: [&[u8]; 2] = [b"a.dds", b"b.dds"];
        let full_a = b"textures\\a.dds";
        let full_b = b"textures\\b.dds";
        let mut block_a = vec![full_a.len() as u8];
        block_a.extend_from_slice(full_a);
        block_a.extend_from_slice(plain);
        let mut block_b = vec![full_b.len() as u8];
        block_b.extend_from_slice(full_b);
        block_b.extend_from_slice(&(packed_src.len() as u32).to_le_bytes());
        block_b.extend_from_slice(&lz4);

        let data_start: u32 = 36 + 24 + (1 + folder.len() as u32 + 1 + 2 * 16) + 6 + 6;
        let mut d = Vec::new();
        d.extend_from_slice(b"BSA\0");
        d.extend_from_slice(&105u32.to_le_bytes());
        d.extend_from_slice(&36u32.to_le_bytes());
        d.extend_from_slice(&0x103u32.to_le_bytes()); // names + embed names, stored by default
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&(folder.len() as u32 + 1).to_le_bytes());
        d.extend_from_slice(&12u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&folder_hash("textures").to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0u64.to_le_bytes());
        d.push(folder.len() as u8 + 1);
        d.extend_from_slice(folder);
        d.push(0);
        d.extend_from_slice(&file_name_hash("a.dds").to_le_bytes());
        d.extend_from_slice(&(block_a.len() as u32).to_le_bytes());
        d.extend_from_slice(&data_start.to_le_bytes());
        d.extend_from_slice(&file_name_hash("b.dds").to_le_bytes());
        d.extend_from_slice(&((block_b.len() as u32) | BSA_SIZE_COMPRESSION_BIT).to_le_bytes());
        d.extend_from_slice(&(data_start + block_a.len() as u32).to_le_bytes());
        for n in names {
            d.extend_from_slice(n);
            d.push(0);
        }
        assert_eq!(d.len() as u32, data_start);
        d.extend_from_slice(&block_a);
        d.extend_from_slice(&block_b);

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tex.bsa");
        std::fs::write(&path, &d).unwrap();
        let archive = Archive::from_bytes(Path::new("tex.bsa"), d).unwrap();
        assert!(archive.index().records[0].name_embedded);
        assert_eq!(
            archive.index().records[1].uncompressed_size,
            packed_src.len() as u32
        );
        assert_eq!(archive.read("textures/a.dds").unwrap(), plain);
        assert_eq!(archive.read("textures/b.dds").unwrap(), packed_src);
        let disk = Archive::open(&path).unwrap();
        assert_eq!(disk.read("textures/a.dds").unwrap(), plain);
        assert_eq!(disk.read("textures/b.dds").unwrap(), packed_src);
    }

    #[test]
    fn a_record_past_the_end_is_reported_not_panicked() {
        let mut d = sse_bsa(&[("meshes", "a.nif", b"0123456789")]);
        d.truncate(d.len() - 5);
        let archive = Archive::from_bytes(Path::new("cut.bsa"), d).unwrap();
        let err = archive.read("meshes/a.nif").unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }), "{err}");
    }

    #[test]
    fn absurd_counts_in_the_header_are_refused() {
        let mut data = Vec::new();
        data.extend_from_slice(b"BSA\0");
        data.extend_from_slice(&105u32.to_le_bytes());
        data.extend_from_slice(&36u32.to_le_bytes());
        data.extend_from_slice(&0x03u32.to_le_bytes());
        data.extend_from_slice(&u32::MAX.to_le_bytes()); // folders
        data.extend_from_slice(&u32::MAX.to_le_bytes()); // files
        data.extend_from_slice(&[0u8; 12]);
        let err = index_archive_in(&data, Path::new("huge.bsa")).unwrap_err();
        assert!(err.to_string().contains("more than the archive can hold"));

        let mut ba2 = Vec::new();
        ba2.extend_from_slice(b"BTDX");
        ba2.extend_from_slice(&1u32.to_le_bytes());
        ba2.extend_from_slice(b"GNRL");
        ba2.extend_from_slice(&u32::MAX.to_le_bytes());
        ba2.extend_from_slice(&0u64.to_le_bytes());
        assert!(index_archive_in(&ba2, Path::new("huge.ba2")).is_err());
    }

    #[test]
    fn tst_08_ba2_dx10_raw_chunk_rebuilds_a_dds_file() {
        // One DX10 record, one uncompressed chunk of 16 bytes.
        let pixels = [0xABu8; 16];
        let mut d = Vec::new();
        d.extend_from_slice(b"BTDX");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"DX10");
        d.extend_from_slice(&1u32.to_le_bytes());
        // Header 24 + record (24 + 1 chunk * 24) = 72, name table there.
        let record_len = 24u64 + 24;
        let name = b"textures/t.dds";
        let name_table = 24 + record_len;
        let data_at = name_table + 2 + name.len() as u64;
        d.extend_from_slice(&name_table.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // name hash
        d.extend_from_slice(b"dds\0");
        d.extend_from_slice(&0u32.to_le_bytes()); // dir hash
        d.push(0); // unknown
        d.push(1); // chunks
        d.extend_from_slice(&24u16.to_le_bytes()); // chunk header size
        d.extend_from_slice(&2u16.to_le_bytes()); // height
        d.extend_from_slice(&2u16.to_le_bytes()); // width
        d.push(1); // mips
        d.push(98); // DXGI_FORMAT_BC7_UNORM
        d.extend_from_slice(&0u16.to_le_bytes()); // tile mode
        d.extend_from_slice(&data_at.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // packed
        d.extend_from_slice(&16u32.to_le_bytes()); // unpacked
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
        assert_eq!(d.len() as u64, name_table);
        d.extend_from_slice(&(name.len() as u16).to_le_bytes());
        d.extend_from_slice(name);
        d.extend_from_slice(&pixels);

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("t.ba2");
        std::fs::write(&path, &d).unwrap();
        let archive = Archive::from_bytes(Path::new("t.ba2"), d).unwrap();
        let dds = archive.read("textures/t.dds").unwrap();
        assert_eq!(
            Archive::open(&path)
                .unwrap()
                .read("textures/t.dds")
                .unwrap(),
            dds
        );
        assert_eq!(&dds[..4], b"DDS ");
        assert_eq!(dds.len(), 4 + 124 + 20 + 16);
        assert_eq!(&dds[dds.len() - 16..], &pixels);
        // dxgiFormat sits right after the 128-byte DDS header.
        assert_eq!(u32::from_le_bytes(dds[128..132].try_into().unwrap()), 98);
    }

    #[test]
    fn tst_08_ba2_general_reads_a_compressed_record() {
        use std::io::Write;

        let payload = b"BA2 general payload";
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let packed = encoder.finish().unwrap();
        let name = b"meshes/a.nif";
        let name_table = 24 + 36;
        let data_at = name_table + 2 + name.len();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"BTDX");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(b"GNRL");
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(name_table as u64).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"nif\0");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(data_at as u64).to_le_bytes());
        bytes.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(&packed);

        let archive = Archive::from_bytes(Path::new("tst_08.ba2"), bytes).unwrap();
        assert_eq!(archive.read("meshes/a.nif").unwrap(), payload);
    }

    fn tst_08_dx10_archive(payload: &[u8], compressed: bool) -> Vec<u8> {
        use std::io::Write;

        let name = b"textures/t.dds";
        let mut packed = Vec::new();
        if compressed {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(payload).unwrap();
            packed = encoder.finish().unwrap();
        }
        let stored = if compressed { &packed } else { payload };
        let name_table = 24 + 48;
        let data_at = name_table + 2 + name.len();
        let mut d = Vec::new();
        d.extend_from_slice(b"BTDX");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(b"DX10");
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&(name_table as u64).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(b"dds\0");
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&[0, 1]);
        d.extend_from_slice(&24u16.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.push(1);
        d.push(98);
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(&(data_at as u64).to_le_bytes());
        d.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        d.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        d.extend_from_slice(&[0; 4]);
        d.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
        d.extend_from_slice(&(name.len() as u16).to_le_bytes());
        d.extend_from_slice(name);
        d.extend_from_slice(stored);
        d
    }

    #[test]
    fn tst_08_ba2_dx10_compressed_chunk_rebuilds_with_full_payload() {
        let pixels = [0x5Au8; 16];
        let bytes = tst_08_dx10_archive(&pixels, true);
        let archive = Archive::from_bytes(Path::new("tst_08.ba2"), bytes).unwrap();
        let dds = archive.read("textures/t.dds").unwrap();
        assert_eq!(&dds[..4], b"DDS ");
        assert_eq!(u32::from_le_bytes(dds[128..132].try_into().unwrap()), 98);
        assert_eq!(&dds[148..], &pixels);
    }

    #[test]
    fn tst_08_ba2_dx10_truncated_chunk_returns_an_error() {
        let pixels = [0x5Au8; 16];
        let mut bytes = tst_08_dx10_archive(&pixels, false);
        bytes.pop();
        let archive = Archive::from_bytes(Path::new("tst_08.ba2"), bytes).unwrap();
        assert!(matches!(
            archive.read("textures/t.dds"),
            Err(Error::Truncated { .. })
        ));
    }

    #[test]
    fn tst_08_ba2_dx10_invalid_chunk_sentinel_is_rejected() {
        let pixels = [0x5Au8; 16];
        let mut bytes = tst_08_dx10_archive(&pixels, false);
        let sentinel_offset = 24 + 24 + 20;
        bytes[sentinel_offset..sentinel_offset + 4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            index_archive_in(&bytes, Path::new("tst_08.ba2")),
            Err(Error::Format { .. })
        ));
    }

    #[test]
    fn tst_08_ba2_dx10_invalid_chunk_header_size_is_rejected() {
        let pixels = [0x5Au8; 16];
        let mut bytes = tst_08_dx10_archive(&pixels, false);
        bytes[38..40].copy_from_slice(&20u16.to_le_bytes());
        assert!(matches!(
            index_archive_in(&bytes, Path::new("tst_08.ba2")),
            Err(Error::Format { .. })
        ));
    }

    /// Wraps a reader and counts the bytes actually pulled through it.
    struct Counting<R> {
        inner: R,
        read: u64,
    }

    impl<R: Read> Read for Counting<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.inner.read(buf)?;
            self.read += n as u64;
            Ok(n)
        }
    }

    impl<R: Seek> Seek for Counting<R> {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    // Listing an archive's contents must not read the archive. Bytes pulled,
    // not memory sampled, so a parallel test run cannot make it flaky.
    #[test]
    fn indexing_reads_the_tables_not_the_payload() {
        let (mut bytes, _) = compressed_bsa(b"payload");
        bytes.resize(64 * 1024 * 1024, 0u8);

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("big.bsa");
        std::fs::write(&path, &bytes).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let len = file.metadata().unwrap().len();
        let mut counting = Counting {
            inner: file,
            read: 0,
        };
        let index = index_from(&mut counting, len, &path, false).unwrap();

        assert_eq!(index.files, vec!["meshes/a.nif".to_string()]);
        assert!(
            counting.read < 64 * 1024,
            "indexing a {} MiB archive read {} bytes, it should only need the header and tables",
            len / 1048576,
            counting.read
        );
    }
}
