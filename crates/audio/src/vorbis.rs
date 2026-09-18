// SPDX-License-Identifier: GPL-3.0-or-later

//! Ogg Vorbis, decoded by `lewton`.
//!
//! Nothing here implements Vorbis. `lewton` is a pure Rust decoder that costs
//! three crates in the tree, so the engine gains an open alternative to xWMA
//! without a C toolchain and without a decoder of our own to maintain.

use crate::error::{Error, Result};
use crate::pcm::Pcm;
use lewton::inside_ogg::OggStreamReader;

/// Decodes a whole Ogg Vorbis file.
///
/// # Errors
///
/// [`Error::Decode`] when the headers or any packet fail to decode.
pub fn decode(bytes: &[u8]) -> Result<Pcm> {
    let mut reader = OggStreamReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Decode(format!("vorbis headers: {e}")))?;
    let channels = u16::from(reader.ident_hdr.audio_channels);
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = reader
            .read_dec_packet_itl()
            .map_err(|e| Error::Decode(format!("vorbis packet: {e}")))?;
        match packet {
            Some(frames) => samples.extend(frames.into_iter().map(from_i16)),
            None => break,
        }
    }
    Ok(Pcm {
        sample_rate,
        channels,
        samples,
    })
}

fn from_i16(s: i16) -> f32 {
    f32::from(s) / 32768.0
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn junk_is_a_decode_error_not_a_panic() {
        assert!(decode(b"OggS not really").is_err());
    }
}
