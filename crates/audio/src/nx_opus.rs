// SPDX-License-Identifier: GPL-3.0-or-later

//! Switch Skyrim Opus, using Skyrim-NX-Toolkit's `NXOPUS2FUZ` wrapper and
//! VGAudio's standard `NxOpusReader` and `NxOpusWriter` container layout.
//! Packet decoding is provided by the existing `opus-decoder` dependency.

use crate::error::{Error, Result};
use crate::fuz::NX_OPUS_SIGNATURE;
use crate::pcm::Pcm;
use opus_decoder::OpusDecoder;

/// Maximum decoded sample storage, about 11.6 minutes of 48 kHz stereo.
pub const MAX_PCM_BYTES: usize = 256 * 1024 * 1024;

/// Decodes a Switch Skyrim audio payload, including its 20 byte wrapper.
/// The NX stream supplies the output rate, channel count and encoder delay.
/// The wrapper's rounded duration is metadata, not an exact sample count.
///
/// # Errors
///
/// Returns [`Error::Truncated`] for incomplete headers or packets,
/// [`Error::Malformed`] for invalid fields or unsupported NX variants, and
/// [`Error::Decode`] when a packet fails, PCM exceeds [`MAX_PCM_BYTES`], or
/// sample storage cannot be allocated.
pub fn decode(bytes: &[u8]) -> Result<Pcm> {
    decode_with_limit(bytes, MAX_PCM_BYTES)
}

fn decode_with_limit(bytes: &[u8], max_pcm_bytes: usize) -> Result<Pcm> {
    let header = take(bytes, 0, 20)?;
    if header[..4] != NX_OPUS_SIGNATURE || le_u32(header, 12)? != 20 {
        return Err(malformed("invalid Skyrim wrapper signature or header size"));
    }
    let channels = le_u32(header, 8)?;
    let stream = take(bytes, 20, le_u32(header, 16)? as usize)?;
    if stream.len() != bytes.len() - 20 {
        return Err(malformed("bytes follow the declared Skyrim payload"));
    }
    let nx = take(stream, 0, 32)?;
    if le_u32(nx, 0)? != 0x8000_0001 || le_u32(nx, 4)? != 24 || nx[8] != 0 {
        return Err(malformed("expected a version 0 standard NX Opus header"));
    }
    if !(1..=2).contains(&channels) || u32::from(nx[9]) != channels {
        return Err(malformed("channel count must agree and be mono or stereo"));
    }
    let sample_rate = le_u32(nx, 12)?;
    if !matches!(sample_rate, 8000 | 12000 | 16000 | 24000 | 48000) {
        return Err(malformed("invalid Opus sample rate"));
    }
    let offset = le_u32(nx, 16)? as usize;
    if offset < 32 {
        return Err(malformed("NX data overlaps its header"));
    }
    let data_header = take(stream, offset, 8)?;
    if le_u32(data_header, 0)? != 0x8000_0004 {
        return Err(malformed("invalid NX data chunk type"));
    }
    let mut packets = take(stream, offset + 8, le_u32(data_header, 4)? as usize)?;
    if packets.len() != stream.len() - offset - 8 {
        return Err(malformed("bytes follow the declared NX data chunk"));
    }
    let mut skip = le_u32(nx, 28)? as usize;
    let channels = channels as usize;
    let mut decoder = OpusDecoder::new(sample_rate, channels)
        .map_err(|e| Error::Decode(format!("NX opus decoder: {e}")))?;
    let mut frame = vec![0.0; OpusDecoder::MAX_FRAME_SIZE_48K * channels];
    let mut samples: Vec<f32> = Vec::new();
    let max_samples = max_pcm_bytes / std::mem::size_of::<f32>();
    while !packets.is_empty() {
        let packet_header = take(packets, 0, 8)?;
        let length = u32::from_be_bytes([
            packet_header[0],
            packet_header[1],
            packet_header[2],
            packet_header[3],
        ]) as usize;
        if length == 0 {
            return Err(malformed("empty NX Opus packet"));
        }
        // CONTEXT: The second word is the encoder's final range, not audio.
        let packet = take(packets, 8, length)?;
        let frames = decoder
            .decode_float(packet, &mut frame, false)
            .map_err(|e| Error::Decode(format!("NX opus packet: {e}")))?;
        let dropped = skip.min(frames);
        skip -= dropped;
        let output = &frame[dropped * channels..frames * channels];
        let new_len = samples
            .len()
            .checked_add(output.len())
            .filter(|length| *length <= max_samples)
            .ok_or_else(|| Error::Decode("NX Opus decoded PCM exceeds byte limit".into()))?;
        if samples.capacity() < new_len {
            let capacity = samples
                .capacity()
                .saturating_mul(2)
                .max(new_len)
                .min(max_samples);
            samples
                .try_reserve_exact(capacity - samples.len())
                .map_err(|e| Error::Decode(format!("NX Opus PCM allocation: {e}")))?;
        }
        samples.extend_from_slice(output);
        packets = &packets[8 + length..];
    }
    if skip != 0 || samples.is_empty() {
        return Err(malformed(
            "NX stream has no samples after its encoder delay",
        ));
    }
    Ok(Pcm {
        sample_rate,
        channels: channels as u16,
        samples,
    })
}

fn malformed(why: &str) -> Error {
    Error::Malformed {
        what: "NX Opus",
        why: why.to_owned(),
    }
}

fn take(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| malformed("offset plus length overflows"))?;
    bytes.get(offset..end).ok_or(Error::Truncated {
        what: "NX Opus",
        needed: end,
        have: bytes.len(),
    })
}

fn le_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let word = take(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([word[0], word[1], word[2], word[3]]))
}

#[cfg(test)]
#[path = "nx_opus_tests.rs"]
mod tests;
