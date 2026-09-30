// SPDX-License-Identifier: GPL-3.0-or-later

//! In-memory writer for compressed BSA archives.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use crate::archive::BsaVersion;
use crate::error::{Error, Result};
use crate::hash::{file_name_hash, folder_hash};

const ARCHIVE_FLAGS: u32 = 0x01 | 0x02 | 0x04;
const SIZE_COMPRESSION_BIT: u32 = 1 << 30;

/// A path and uncompressed payload to include in a BSA.
#[derive(Debug, Clone, Copy)]
pub struct BsaFile<'a> {
    /// Data-relative path, using either slash direction.
    pub path: &'a str,
    /// File bytes before archive compression.
    pub bytes: &'a [u8],
}

struct Entry<'a> {
    path: String,
    folder: String,
    name: String,
    folder_hash: u64,
    name_hash: u64,
    folder_name: Vec<u8>,
    file_name: Vec<u8>,
    bytes: &'a [u8],
    packed: Vec<u8>,
}

/// Write a compressed BSA archive in memory.
///
/// Files are grouped by folder hash and sorted by file hash. Equal hashes use
/// normalized paths as a stable tie-breaker. BSA v103 and v104 use zlib, and
/// v105 uses an LZ4 frame. Names are written in Windows-1252.
///
/// # Errors
///
/// Returns an error when a path is invalid, a name cannot be represented in
/// Windows-1252, an archive field exceeds its BSA size limit, or compression
/// fails.
///
/// # Panics
///
/// This function does not panic for malformed paths or oversized inputs.
pub fn write_bsa(files: &[BsaFile<'_>], version: BsaVersion) -> Result<Vec<u8>> {
    let mut seen = HashSet::with_capacity(files.len());
    let mut entries = Vec::with_capacity(files.len());

    for file in files {
        let path = file.path.replace('/', "\\");
        if path.is_empty()
            || path.starts_with('\\')
            || path.as_bytes().get(1) == Some(&b':')
            || path.ends_with('\\')
            || path.contains('\0')
            || path
                .split('\\')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(invalid(format!("Invalid BSA path {:?}", file.path)));
        }
        if !seen.insert(path.to_lowercase()) {
            return Err(invalid(format!("Duplicate BSA path {:?}", file.path)));
        }

        let (folder, name) = match path.rsplit_once('\\') {
            Some((folder, name)) => (folder.to_owned(), name.to_owned()),
            None => (String::new(), path.clone()),
        };
        let folder_bytes = encode_name(&folder)?;
        let name_bytes = encode_name(&name)?;
        if folder_bytes.len() + 1 > u8::MAX as usize {
            return Err(invalid(format!(
                "BSA folder name is too long in {:?}",
                file.path
            )));
        }
        let mut folder_name = Vec::with_capacity(folder_bytes.len() + 2);
        folder_name.push((folder_bytes.len() + 1) as u8);
        folder_name.extend_from_slice(&folder_bytes);
        folder_name.push(0);

        let mut file_name = name_bytes;
        file_name.push(0);
        let packed = compress(file.bytes, version)?;
        entries.push(Entry {
            path,
            folder_hash: folder_hash(&folder),
            name_hash: file_name_hash(&name),
            folder,
            name,
            folder_name,
            file_name,
            bytes: file.bytes,
            packed,
        });
    }

    entries.sort_by(|a, b| {
        (
            a.folder_hash,
            a.folder.to_lowercase(),
            a.path.to_lowercase(),
            &a.path,
        )
            .cmp(&(
                b.folder_hash,
                b.folder.to_lowercase(),
                b.path.to_lowercase(),
                &b.path,
            ))
    });

    let mut folders: Vec<(u64, String, Vec<usize>)> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        if let Some((hash, folder, indices)) = folders.last_mut() {
            if *hash == entry.folder_hash && folder.to_lowercase() == entry.folder.to_lowercase() {
                indices.push(index);
                continue;
            }
        }
        folders.push((entry.folder_hash, entry.folder.clone(), vec![index]));
    }

    for (_, _, indices) in &mut folders {
        indices.sort_by(|left, right| {
            let a = &entries[*left];
            let b = &entries[*right];
            (a.name_hash, a.name.to_lowercase(), &a.name).cmp(&(
                b.name_hash,
                b.name.to_lowercase(),
                &b.name,
            ))
        });
    }

    let folder_count = u32::try_from(folders.len()).map_err(|_| invalid("Too many BSA folders"))?;
    let file_count = u32::try_from(entries.len()).map_err(|_| invalid("Too many BSA files"))?;
    let total_folder_name_len = folders
        .iter()
        .try_fold(0u32, |total, (_, _, indices)| {
            let folder_size = entries[indices[0]].folder_name.len() - 1;
            total.checked_add(u32::try_from(folder_size).ok()?)
        })
        .ok_or_else(|| invalid("BSA folder names exceed the header limit"))?;
    let total_file_name_len = entries
        .iter()
        .try_fold(0u32, |total, entry| {
            total.checked_add(u32::try_from(entry.file_name.len()).ok()?)
        })
        .ok_or_else(|| invalid("BSA file names exceed the header limit"))?;

    let header_record_size: u64 = match version {
        BsaVersion::SkyrimSe => 24,
        BsaVersion::Oblivion | BsaVersion::Fo3Skyrim => 16,
    };
    let mut output = Vec::new();
    output.extend_from_slice(b"BSA\0");
    put_u32(
        &mut output,
        match version {
            BsaVersion::Oblivion => 103,
            BsaVersion::Fo3Skyrim => 104,
            BsaVersion::SkyrimSe => 105,
        },
    );
    put_u32(&mut output, 0x24);
    put_u32(&mut output, ARCHIVE_FLAGS);
    put_u32(&mut output, folder_count);
    put_u32(&mut output, file_count);
    put_u32(&mut output, total_folder_name_len);
    put_u32(&mut output, total_file_name_len);
    put_u32(&mut output, 0);

    for (folder_index, (hash, _, indices)) in folders.iter().enumerate() {
        put_u64(&mut output, *hash);
        put_u32(
            &mut output,
            u32::try_from(indices.len()).map_err(|_| invalid("Too many files in BSA folder"))?,
        );
        if version == BsaVersion::SkyrimSe {
            put_u32(&mut output, 0);
        }
        let slot = output.len();
        if version == BsaVersion::SkyrimSe {
            put_u64(&mut output, 0);
        } else {
            put_u32(&mut output, 0);
        }

        let prior_records: usize = folders[..folder_index]
            .iter()
            .map(|(_, _, prior)| {
                let folder_name_length = entries[prior[0]].folder_name.len();
                folder_name_length + prior.len() * 16
            })
            .sum();
        let remaining_headers = (folders.len() - folder_index) as u64 * header_record_size;
        let folder_offset =
            36u64 + remaining_headers + u64::from(total_file_name_len) + prior_records as u64;
        let offset =
            u32::try_from(folder_offset).map_err(|_| invalid("BSA folder offset exceeds 4 GiB"))?;
        if version == BsaVersion::SkyrimSe {
            output[slot..slot + 8].copy_from_slice(&folder_offset.to_le_bytes());
        } else {
            output[slot..slot + 4].copy_from_slice(&offset.to_le_bytes());
        }
    }

    let mut payload_offset_slots = vec![0usize; entries.len()];
    for (_, _, indices) in &folders {
        output.extend_from_slice(&entries[indices[0]].folder_name);
        for index in indices {
            let entry = &entries[*index];
            put_u64(&mut output, entry.name_hash);
            let raw_size = entry
                .packed
                .len()
                .checked_add(4)
                .and_then(|size| u32::try_from(size).ok())
                .filter(|size| *size < SIZE_COMPRESSION_BIT)
                .ok_or_else(|| invalid(format!("BSA file is too large: {:?}", entry.path)))?;
            put_u32(&mut output, raw_size);
            payload_offset_slots[*index] = output.len();
            put_u32(&mut output, 0);
        }
    }
    for (_, _, indices) in &folders {
        for index in indices {
            output.extend_from_slice(&entries[*index].file_name);
        }
    }
    for (_, _, indices) in &folders {
        for index in indices {
            let entry = &entries[*index];
            let offset = u32::try_from(output.len())
                .map_err(|_| invalid("BSA payload offset exceeds 4 GiB"))?;
            let slot = payload_offset_slots[*index];
            output[slot..slot + 4].copy_from_slice(&offset.to_le_bytes());
            let original_size = u32::try_from(entry.bytes.len())
                .map_err(|_| invalid(format!("BSA file is too large: {:?}", entry.path)))?;
            put_u32(&mut output, original_size);
            output.extend_from_slice(&entry.packed);
        }
    }
    Ok(output)
}

fn encode_name(name: &str) -> Result<Vec<u8>> {
    let (encoded, _, had_errors) = encoding_rs::WINDOWS_1252.encode(name);
    if had_errors {
        return Err(invalid(format!(
            "BSA name is not representable in Windows-1252: {name:?}"
        )));
    }
    Ok(encoded.into_owned())
}

fn compress(bytes: &[u8], version: BsaVersion) -> Result<Vec<u8>> {
    match version {
        BsaVersion::Oblivion | BsaVersion::Fo3Skyrim => {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
            encoder.write_all(bytes)?;
            Ok(encoder.finish()?)
        }
        BsaVersion::SkyrimSe => {
            let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
            encoder.write_all(bytes)?;
            encoder
                .finish()
                .map_err(|error| std::io::Error::other(error.to_string()).into())
        }
    }
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn invalid(message: impl Into<String>) -> Error {
    Error::format(message, Path::new("<memory>"))
}
