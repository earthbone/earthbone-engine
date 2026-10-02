#![no_main]

//! Grammar driven archive fuzzing, aimed at the read path rather than the
//! index.
//!
//! `fuzz-bsa` feeds raw bytes to `index_archive_in` and stops there, so the
//! record read path behind a successful index is barely reached: random bytes
//! almost never form tables the indexer accepts. This builds an archive whose
//! header and tables are well formed while the sizes, offsets, flags and
//! payload are the fuzzer's, then indexes it and reads every record.
//!
//! The interesting cases are the ones where the tables are plausible and the
//! payload does not agree with them, which is what a truncated download or a
//! corrupt patch actually looks like. The assertion is that a read either
//! returns bytes or an error, never a panic, and that when a record declares
//! an unpacked length the bytes handed back are that long. That second half is
//! the invariant a defect on 2026-09-18 broke, when a short decompression came
//! back as success.

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::io::Write;
use std::path::Path;

#[derive(Arbitrary, Debug)]
struct Entry {
    name: u8,
    compressed: bool,
    payload: Vec<u8>,
    /// Written as the declared unpacked length instead of the true one when
    /// set, which is exactly the corruption the size check exists for.
    lie_about_size: Option<u16>,
}

#[derive(Arbitrary, Debug)]
struct Plan {
    format: ArchiveKind,
    sse: bool,
    archive_compressed_flag: bool,
    entries: Vec<Entry>,
    truncate_by: u16,
}

#[derive(Arbitrary, Debug)]
enum ArchiveKind {
    Bsa,
    Ba2General,
    Ba2Dx10,
}

fn pack(payload: &[u8], sse: bool) -> Vec<u8> {
    if sse {
        let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
        let _ = enc.write_all(payload);
        enc.finish().unwrap_or_default()
    } else {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = enc.write_all(payload);
        enc.finish().unwrap_or_default()
    }
}

fn fuzz_bsa(plan: &Plan) {
    let entries: Vec<&Entry> = plan.entries.iter().take(6).collect();
    if entries.is_empty() {
        return;
    }

    let folder = b"meshes\0";
    let version: u32 = if plan.sse { 105 } else { 104 };
    let names: Vec<Vec<u8>> = entries
        .iter()
        .map(|e| format!("f{}.nif\0", e.name).into_bytes())
        .collect();
    let names_len: u32 = names.iter().map(|n| n.len() as u32).sum();

    // 36 header, 16 folder record, then the folder block and the file table.
    let data_offset: u32 =
        36 + 16 + (1 + folder.len() as u32 + 16 * entries.len() as u32) + names_len;

    let mut payloads: Vec<(Vec<u8>, u32, bool)> = Vec::new();
    for e in &entries {
        let body = if e.payload.len() > 4096 {
            &e.payload[..4096]
        } else {
            &e.payload[..]
        };
        if e.compressed {
            let packed = pack(body, plan.sse);
            let declared = e
                .lie_about_size
                .map_or(body.len() as u32, |n| body.len() as u32 + u32::from(n));
            let mut block = Vec::with_capacity(4 + packed.len());
            block.extend_from_slice(&declared.to_le_bytes());
            block.extend_from_slice(&packed);
            payloads.push((block, declared, true));
        } else {
            payloads.push((body.to_vec(), body.len() as u32, false));
        }
    }

    let mut d = Vec::new();
    d.extend_from_slice(b"BSA\0");
    d.extend_from_slice(&version.to_le_bytes());
    d.extend_from_slice(&36u32.to_le_bytes());
    let archive_flags: u32 = 0x1 | 0x2 | if plan.archive_compressed_flag { 0x4 } else { 0 };
    d.extend_from_slice(&archive_flags.to_le_bytes());
    d.extend_from_slice(&1u32.to_le_bytes());
    d.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    d.extend_from_slice(&(folder.len() as u32).to_le_bytes());
    d.extend_from_slice(&names_len.to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.extend_from_slice(&0u64.to_le_bytes());
    d.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d.push(folder.len() as u8);
    d.extend_from_slice(folder);

    let mut at = data_offset;
    for (i, (block, _, compressed)) in payloads.iter().enumerate() {
        d.extend_from_slice(&(i as u64).to_le_bytes());
        // The compressed bit on a record is the inverse of the archive flag.
        let mut size = block.len() as u32;
        if *compressed != plan.archive_compressed_flag {
            size |= 1 << 30;
        }
        d.extend_from_slice(&size.to_le_bytes());
        d.extend_from_slice(&at.to_le_bytes());
        at += block.len() as u32;
    }
    for n in &names {
        d.extend_from_slice(n);
    }
    for (block, _, _) in &payloads {
        d.extend_from_slice(block);
    }

    let cut = usize::from(plan.truncate_by).min(d.len());
    d.truncate(d.len() - cut);

    let path = Path::new("fuzz.bsa");
    let Ok(index) = soe_bsa::index_archive_in(&d, path) else {
        return;
    };
    let Ok(archive) = soe_bsa::Archive::from_bytes(path, d) else {
        return;
    };
    for (i, record) in index.records.iter().enumerate() {
        let Ok(bytes) = archive.read_record(i) else {
            continue;
        };
        if record.uncompressed_size > 0 {
            assert_eq!(
                bytes.len(),
                record.uncompressed_size as usize,
                "a record declaring {} bytes handed back {}",
                record.uncompressed_size,
                bytes.len()
            );
        }
    }
}

fn fuzz_ba2(plan: &Plan, dx10: bool) {
    let entries: Vec<&Entry> = plan.entries.iter().take(6).collect();
    if entries.is_empty() {
        return;
    }

    let names: Vec<Vec<u8>> = entries
        .iter()
        .map(|entry| format!("textures/f{}.dds", entry.name).into_bytes())
        .collect();
    let name_table_len: usize = names.iter().map(|name| 2 + name.len()).sum();
    let record_size = if dx10 { 48 } else { 36 };
    let records_end = 24 + record_size * entries.len();
    let data_start = records_end + name_table_len;
    let mut chunks = Vec::with_capacity(entries.len());
    for entry in &entries {
        let body = &entry.payload[..entry.payload.len().min(4096)];
        let packed = if entry.compressed {
            let mut encoder =
                flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            let _ = encoder.write_all(body);
            encoder.finish().unwrap_or_default()
        } else {
            Vec::new()
        };
        chunks.push((body.to_vec(), packed));
    }

    let mut d = Vec::new();
    d.extend_from_slice(b"BTDX");
    d.extend_from_slice(&1u32.to_le_bytes());
    d.extend_from_slice(if dx10 { b"DX10" } else { b"GNRL" });
    d.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    d.extend_from_slice(&(records_end as u64).to_le_bytes());

    let mut data_at = data_start as u64;
    for (entry, (body, packed)) in entries.iter().zip(&chunks) {
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(b"dds\0");
        d.extend_from_slice(&0u32.to_le_bytes());
        if dx10 {
            d.push(0);
            d.push(1);
            d.extend_from_slice(&24u16.to_le_bytes());
            d.extend_from_slice(&4u16.to_le_bytes());
            d.extend_from_slice(&4u16.to_le_bytes());
            d.push(1);
            d.push(98); // DXGI_FORMAT_BC7_UNORM
            d.extend_from_slice(&0u16.to_le_bytes());
            d.extend_from_slice(&data_at.to_le_bytes());
            d.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            d.extend_from_slice(&(body.len() as u32).to_le_bytes());
            d.extend_from_slice(&0u16.to_le_bytes());
            d.extend_from_slice(&0u16.to_le_bytes());
            d.extend_from_slice(&0xBAAD_F00Du32.to_le_bytes());
        } else {
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&data_at.to_le_bytes());
            d.extend_from_slice(&(packed.len() as u32).to_le_bytes());
            d.extend_from_slice(&(body.len() as u32).to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
        }
        data_at += if entry.compressed {
            packed.len() as u64
        } else {
            body.len() as u64
        };
    }
    for name in &names {
        d.extend_from_slice(&(name.len() as u16).to_le_bytes());
        d.extend_from_slice(name);
    }
    for (entry, (body, packed)) in entries.iter().zip(&chunks) {
        if entry.compressed {
            d.extend_from_slice(packed);
        } else {
            d.extend_from_slice(body);
        }
    }

    let cut = usize::from(plan.truncate_by).min(d.len());
    d.truncate(d.len() - cut);
    let path = Path::new("fuzz.ba2");
    let Ok(index) = soe_bsa::index_archive_in(&d, path) else {
        return;
    };
    let Ok(archive) = soe_bsa::Archive::from_bytes(path, d) else {
        return;
    };
    for (i, record) in index.records.iter().enumerate() {
        let Ok(bytes) = archive.read_record(i) else {
            continue;
        };
        if dx10 {
            assert!(
                bytes.starts_with(b"DDS "),
                "DX10 read must rebuild a DDS header"
            );
            assert!(bytes.len() >= 148, "rebuilt DDS must include DX10 header");
            assert_eq!(u32::from_le_bytes(bytes[128..132].try_into().unwrap()), 98);
            assert_eq!(
                &bytes[148..],
                &chunks[i].0,
                "rebuilt DDS payload must contain the decompressed chunk"
            );
        } else if record.uncompressed_size > 0 {
            assert_eq!(bytes.len(), record.uncompressed_size as usize);
        }
    }
}

fuzz_target!(|plan: Plan| match plan.format {
    ArchiveKind::Bsa => fuzz_bsa(&plan),
    ArchiveKind::Ba2General => fuzz_ba2(&plan, false),
    ArchiveKind::Ba2Dx10 => fuzz_ba2(&plan, true),
});
