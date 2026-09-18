// SPDX-License-Identifier: GPL-3.0-or-later

//! Ogg Opus, demuxed by the `ogg` crate and decoded by `opus-decoder`.
//!
//! Neither the container nor the codec is implemented here. `opus-decoder` is
//! a pure Rust port that costs one crate plus `thiserror`, which the
//! workspace already carries, and it avoids the libopus C dependency the
//! other bindings need. The header fields below are RFC 7845 section 5.1.

use crate::error::{Error, Result};
use crate::pcm::Pcm;
use ogg::PacketReader;
use opus_decoder::OpusDecoder;

/// Opus always decodes at 48 kHz, whatever the source was recorded at.
pub const SAMPLE_RATE: u32 = 48_000;

/// The identification header every Ogg Opus stream opens with.
const HEAD_MAGIC: &[u8; 8] = b"OpusHead";

/// Decodes a whole Ogg Opus file, dropping the encoder delay the header
/// declares so the result starts where the audio does.
///
/// # Errors
///
/// [`Error::Malformed`] when the identification header is missing or short,
/// [`Error::Unsupported`] for a multistream mapping, and [`Error::Decode`]
/// when a packet fails.
pub fn decode(bytes: &[u8]) -> Result<Pcm> {
    let mut reader = PacketReader::new(std::io::Cursor::new(bytes));

    let head = reader
        .read_packet()
        .map_err(|e| Error::Decode(format!("ogg page: {e}")))?
        .ok_or(Error::Malformed {
            what: "Ogg Opus",
            why: "the stream has no packets".to_string(),
        })?;
    if head.data.len() < 19 || !head.data.starts_with(HEAD_MAGIC) {
        return Err(Error::Malformed {
            what: "Ogg Opus",
            why: "the first packet is not an OpusHead".to_string(),
        });
    }
    let channels = head.data[9];
    let pre_skip = u16::from_le_bytes([head.data[10], head.data[11]]);
    // Mapping family 0 is mono or stereo in one stream, which is what a mod
    // ships. Anything else needs the multistream decoder and a channel map.
    if head.data[18] != 0 || channels == 0 || channels > 2 {
        return Err(Error::Unsupported(crate::error::Unsupported::Opus));
    }

    // The comment header is next and carries no audio.
    reader
        .read_packet()
        .map_err(|e| Error::Decode(format!("ogg page: {e}")))?;

    let mut decoder = OpusDecoder::new(SAMPLE_RATE, channels as usize)
        .map_err(|e| Error::Decode(format!("opus decoder: {e}")))?;
    let mut frame = vec![0f32; OpusDecoder::MAX_FRAME_SIZE_48K * channels as usize];
    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = reader
            .read_packet()
            .map_err(|e| Error::Decode(format!("ogg page: {e}")))?;
        let Some(packet) = packet else { break };
        if packet.data.is_empty() {
            continue;
        }
        let frames = decoder
            .decode_float(&packet.data, &mut frame, false)
            .map_err(|e| Error::Decode(format!("opus packet: {e}")))?;
        samples.extend_from_slice(&frame[..frames * channels as usize]);
    }

    let skip = pre_skip as usize * channels as usize;
    if skip >= samples.len() {
        samples.clear();
    } else {
        samples.drain(..skip);
    }

    Ok(Pcm {
        sample_rate: SAMPLE_RATE,
        channels: u16::from(channels),
        samples,
    })
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn junk_is_an_error_not_a_panic() {
        assert!(decode(b"OggS nonsense").is_err());
        assert!(decode(&[]).is_err());
    }
}
