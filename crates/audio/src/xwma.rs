// SPDX-License-Identifier: GPL-3.0-or-later

//! The xWMA container, which is what sits inside every vanilla FUZ and every
//! loose `.xwm` file: a RIFF of form `XWMA` holding a `WAVEFORMATEX`, a
//! packet table and the WMA bitstream.
//!
//! The container is read here in full, with no decoder involved, so a caller
//! can learn a line's sample rate, channel count, packet count and exact
//! duration without decoding a byte. Decoding the bitstream itself is
//! [`crate::wma`], which is a separate and optional concern.

use crate::error::{Error, Result};
use crate::riff::{u16_at, u32_at, Riff};

/// `WMAUDIO2`, the tag every vanilla voice line and music track carries.
pub const WMAUDIO2: u16 = 0x0161;

/// `WMAUDIO3`, WMA Pro. Vanilla does not ship it, a mod might.
pub const WMAUDIO3: u16 = 0x0162;

/// The `WAVEFORMATEX` from the `fmt ` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaveFormat {
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub avg_bytes_per_sec: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    /// The `cbSize` tail, kept verbatim so a rebuild is byte exact.
    pub extra: Vec<u8>,
}

impl WaveFormat {
    /// The smallest `fmt ` chunk, without the `cbSize` field.
    pub const MIN_LEN: usize = 16;

    /// Reads a `WAVEFORMATEX`.
    ///
    /// # Errors
    ///
    /// [`Error::Truncated`] when the chunk is shorter than the fixed fields,
    /// and [`Error::Malformed`] when `cbSize` claims more tail than the chunk
    /// holds.
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (Some(format_tag), Some(channels)) = (u16_at(data, 0), u16_at(data, 2)) else {
            return Err(Error::Truncated {
                what: "fmt chunk",
                needed: Self::MIN_LEN,
                have: data.len(),
            });
        };
        let (Some(sample_rate), Some(avg_bytes_per_sec)) = (u32_at(data, 4), u32_at(data, 8))
        else {
            return Err(Error::Truncated {
                what: "fmt chunk",
                needed: Self::MIN_LEN,
                have: data.len(),
            });
        };
        let (Some(block_align), Some(bits_per_sample)) = (u16_at(data, 12), u16_at(data, 14))
        else {
            return Err(Error::Truncated {
                what: "fmt chunk",
                needed: Self::MIN_LEN,
                have: data.len(),
            });
        };
        let extra = match u16_at(data, 16) {
            None => Vec::new(),
            Some(cb) => {
                let cb = cb as usize;
                let from = Self::MIN_LEN + 2;
                match data.get(from..from + cb) {
                    Some(tail) => tail.to_vec(),
                    None => {
                        return Err(Error::Malformed {
                            what: "fmt chunk",
                            why: format!(
                                "cbSize is {cb} but only {} bytes follow",
                                data.len().saturating_sub(from)
                            ),
                        })
                    }
                }
            }
        };
        Ok(Self {
            format_tag,
            channels,
            sample_rate,
            avg_bytes_per_sec,
            block_align,
            bits_per_sample,
            extra,
        })
    }

    /// Bytes one decoded frame occupies, across all channels.
    #[must_use]
    pub fn decoded_frame_bytes(&self) -> u32 {
        let bits = if self.bits_per_sample == 0 {
            16
        } else {
            self.bits_per_sample
        };
        u32::from(self.channels) * u32::from(bits / 8)
    }

    fn to_chunk(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::MIN_LEN + 2 + self.extra.len());
        out.extend_from_slice(&self.format_tag.to_le_bytes());
        out.extend_from_slice(&self.channels.to_le_bytes());
        out.extend_from_slice(&self.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.avg_bytes_per_sec.to_le_bytes());
        out.extend_from_slice(&self.block_align.to_le_bytes());
        out.extend_from_slice(&self.bits_per_sample.to_le_bytes());
        out.extend_from_slice(&(self.extra.len() as u16).to_le_bytes());
        out.extend_from_slice(&self.extra);
        out
    }
}

/// A parsed xWMA file: the format, the packet table and the bitstream.
#[derive(Debug, Clone)]
pub struct Xwma<'a> {
    pub format: WaveFormat,
    /// The `dpds` table: decoded bytes produced after each packet, running
    /// totals, so the last entry is the whole stream. Empty when the file
    /// carries no `dpds` chunk, which a hand-made file may not.
    pub packet_totals: Vec<u32>,
    /// The `data` chunk, still WMA encoded.
    pub data: &'a [u8],
}

impl<'a> Xwma<'a> {
    /// Reads an xWMA file.
    ///
    /// # Errors
    ///
    /// [`Error::NotRiff`] or [`Error::NotXwma`] when this is some other file,
    /// [`Error::MissingChunk`] when `fmt ` or `data` is absent, and whatever
    /// [`WaveFormat::parse`] reports for a bad format chunk.
    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        let riff = Riff::parse(bytes)?;
        if &riff.form != b"XWMA" {
            return Err(Error::NotXwma(
                String::from_utf8_lossy(&riff.form).into_owned(),
            ));
        }
        let fmt = riff.chunk(b"fmt ").ok_or(Error::MissingChunk("fmt "))?;
        let format = WaveFormat::parse(fmt.data)?;
        let data = riff.chunk(b"data").ok_or(Error::MissingChunk("data"))?.data;
        let packet_totals = riff.chunk(b"dpds").map_or_else(Vec::new, |c| {
            let (whole, _tail) = c.data.as_chunks::<4>();
            whole.iter().copied().map(u32::from_le_bytes).collect()
        });
        Ok(Self {
            format,
            packet_totals,
            data,
        })
    }

    /// How many packets the table describes.
    #[must_use]
    pub fn packets(&self) -> usize {
        self.packet_totals.len()
    }

    /// Total decoded bytes, from the last entry of the packet table. Zero
    /// when the file has no table.
    #[must_use]
    pub fn decoded_bytes(&self) -> u64 {
        self.packet_totals.last().copied().map_or(0, u64::from)
    }

    /// Decoded frames, which is decoded bytes over the frame size.
    #[must_use]
    pub fn frames(&self) -> u64 {
        let per = u64::from(self.format.decoded_frame_bytes());
        self.decoded_bytes().checked_div(per).unwrap_or(0)
    }

    /// Exact length in seconds, taken from the packet table rather than from
    /// a bitrate estimate. Zero when the table or the sample rate is missing.
    #[must_use]
    pub fn duration_seconds(&self) -> f64 {
        if self.format.sample_rate == 0 {
            0.0
        } else {
            self.frames() as f64 / f64::from(self.format.sample_rate)
        }
    }

    /// True when this is the `WMAUDIO2` bitstream vanilla ships.
    #[must_use]
    pub fn is_wmaudio2(&self) -> bool {
        self.format.format_tag == WMAUDIO2
    }

    /// Rebuilds a standalone `.xwm` file, which is what a decoder wants when
    /// the bytes came out of a FUZ and have no file of their own.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let fmt = self.format.to_chunk();
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(b"XWMA");
        push_chunk(&mut body, *b"fmt ", &fmt);
        if !self.packet_totals.is_empty() {
            let mut dpds = Vec::with_capacity(self.packet_totals.len() * 4);
            for total in &self.packet_totals {
                dpds.extend_from_slice(&total.to_le_bytes());
            }
            push_chunk(&mut body, *b"dpds", &dpds);
        }
        push_chunk(&mut body, *b"data", self.data);

        let mut out = Vec::with_capacity(8 + body.len());
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}

fn push_chunk(out: &mut Vec<u8>, id: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::{WaveFormat, Xwma, WMAUDIO2};
    use crate::error::Error;

    fn sample() -> Vec<u8> {
        let format = WaveFormat {
            format_tag: WMAUDIO2,
            channels: 1,
            sample_rate: 44100,
            avg_bytes_per_sec: 4000,
            block_align: 1487,
            bits_per_sample: 16,
            extra: Vec::new(),
        };
        let xwma = Xwma {
            format,
            packet_totals: vec![25600, 45056, 88200],
            data: &[0xaa; 32],
        };
        xwma.to_bytes()
    }

    #[test]
    fn a_built_file_parses_back_to_the_same_fields() {
        let bytes = sample();
        let x = Xwma::parse(&bytes).unwrap();
        assert!(x.is_wmaudio2());
        assert_eq!(x.format.sample_rate, 44100);
        assert_eq!(x.format.channels, 1);
        assert_eq!(x.packets(), 3);
        assert_eq!(x.data.len(), 32);
        assert_eq!(x.to_bytes(), bytes);
    }

    #[test]
    fn duration_comes_from_the_last_packet_total() {
        let bytes = sample();
        let x = Xwma::parse(&bytes).unwrap();
        // 88,200 bytes over 2 bytes a frame is 44,100 frames, one second.
        assert_eq!(x.decoded_bytes(), 88200);
        assert_eq!(x.frames(), 44100);
        assert!((x.duration_seconds() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_missing_data_chunk_is_named() {
        let mut body = Vec::new();
        body.extend_from_slice(b"XWMA");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&[0u8; 16]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&body);
        assert!(matches!(
            Xwma::parse(&bytes),
            Err(Error::MissingChunk("data"))
        ));
    }

    #[test]
    fn another_riff_form_is_refused_by_name() {
        let mut bytes = sample();
        bytes[8..12].copy_from_slice(b"WAVE");
        match Xwma::parse(&bytes) {
            Err(Error::NotXwma(form)) => assert_eq!(form, "WAVE"),
            other => panic!("expected NotXwma, got {other:?}"),
        }
    }

    #[test]
    fn a_zero_bit_depth_still_gives_a_frame_size() {
        let f = WaveFormat {
            format_tag: WMAUDIO2,
            channels: 2,
            sample_rate: 44100,
            avg_bytes_per_sec: 0,
            block_align: 0,
            bits_per_sample: 0,
            extra: Vec::new(),
        };
        assert_eq!(f.decoded_frame_bytes(), 4);
    }
}
