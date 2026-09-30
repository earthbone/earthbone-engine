// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoding the WMA bitstream inside an xWMA file.
//!
//! [`Native`] decodes WMA v2 in process through `oxideav-wma`. It follows the
//! packet and synthesized setup rules in FFmpeg's `libavformat/xwma.c`.
//! [`Ffmpeg`] remains an optional out-of-process comparison implementation.
//! The xWMA wrapper is parsed by [`crate::xwma`].

use crate::error::Error;
use crate::error::Result;
use crate::pcm::Pcm;
use crate::xwma::Xwma;

/// Anything that can turn a complete xWMA file into samples.
///
/// The engine holds one of these rather than calling a decoder directly, so
/// an in-process decoder can replace the subprocess later without touching a
/// caller.
pub trait WmaDecoder {
    /// Decodes a standalone xWMA file, the bytes [`Xwma::to_bytes`] produces.
    ///
    /// # Errors
    ///
    /// [`Error::Decode`] when the decoder fails or is not available.
    fn decode_xwma(&self, file: &[u8]) -> Result<Pcm>;
}

/// Decodes an xWMA that has already been parsed, by handing the decoder a
/// rebuilt file. Convenience over [`WmaDecoder::decode_xwma`].
///
/// # Errors
///
/// Whatever the decoder reports.
pub fn decode_with<D: WmaDecoder + ?Sized>(decoder: &D, xwma: &Xwma<'_>) -> Result<Pcm> {
    decoder.decode_xwma(&xwma.to_bytes())
}

/// Decodes WMA v2 packets in process with the pure Rust `oxideav-wma` codec.
#[derive(Debug, Clone, Copy, Default)]
pub struct Native;

impl WmaDecoder for Native {
    fn decode_xwma(&self, file: &[u8]) -> Result<Pcm> {
        use oxideav_core::{CodecId, CodecParameters, CodecTag, Packet, TimeBase};

        let parsed = Xwma::parse(file)?;
        if parsed.format.format_tag != crate::xwma::WMAUDIO2 {
            return Err(Error::Decode(format!(
                "unsupported xWMA codec tag 0x{:04x}",
                parsed.format.format_tag
            )));
        }
        if parsed.format.sample_rate == 0
            || !(1..=2).contains(&parsed.format.channels)
            || parsed.format.block_align == 0
        {
            return Err(Error::Decode("invalid xWMA stream parameters".to_string()));
        }

        let packet_size = usize::from(parsed.format.block_align);
        if parsed.data.is_empty() || parsed.data.len() % packet_size != 0 {
            return Err(Error::Decode(format!(
                "xWMA data length {} is not a nonzero multiple of packet size {packet_size}",
                parsed.data.len()
            )));
        }
        let packet_count = parsed.data.len() / packet_size;
        if !parsed.packet_totals.is_empty() && parsed.packet_totals.len() != packet_count {
            return Err(Error::Decode(format!(
                "xWMA dpds has {} entries for {packet_count} packets",
                parsed.packet_totals.len()
            )));
        }
        if parsed
            .packet_totals
            .windows(2)
            .any(|totals| totals[0] > totals[1])
        {
            return Err(Error::Decode(
                "xWMA dpds totals are not cumulative".to_string(),
            ));
        }
        let expected_samples = if parsed.packet_totals.is_empty() {
            None
        } else {
            let decoded_bytes = parsed.decoded_bytes();
            let frame_bytes = u64::from(parsed.format.decoded_frame_bytes());
            if frame_bytes == 0
                || decoded_bytes == 0
                || decoded_bytes > 128 * 1024 * 1024
                || decoded_bytes % frame_bytes != 0
            {
                return Err(Error::Decode(format!(
                    "xWMA decoded size {decoded_bytes} is outside the supported range"
                )));
            }
            Some(
                usize::try_from(decoded_bytes / 2)
                    .map_err(|_| Error::Decode("xWMA sample count overflows".to_string()))?,
            )
        };

        let mut params = CodecParameters::audio(CodecId::new("wma2"));
        params.sample_rate = Some(parsed.format.sample_rate);
        params.channels = Some(parsed.format.channels);
        params.bit_rate = Some(u64::from(normalized_xwma_bitrate(&parsed)));
        params.tag = Some(CodecTag::WaveFormat(parsed.format.format_tag));
        // xWMA strips WMA2's private setup block. The xWMA demuxer supplies
        // this sentinel flags2 value for the decoder.
        params.extradata = vec![0, 0, 0, 0, 31, 0];
        let mut decoder = oxideav_wma::make_decoder(&params)
            .map_err(|error| Error::Decode(format!("WMA setup: {error}")))?;
        let mut samples = Vec::new();
        for packet in parsed.data.chunks_exact(packet_size) {
            decoder
                .send_packet(&Packet::new(
                    0,
                    TimeBase::from_rate(parsed.format.sample_rate),
                    packet.to_vec(),
                ))
                .map_err(|error| Error::Decode(format!("WMA packet: {error}")))?;
            drain_frames(&mut *decoder, &mut samples, false)?;
        }
        decoder
            .flush()
            .map_err(|error| Error::Decode(format!("WMA flush: {error}")))?;
        drain_frames(&mut *decoder, &mut samples, true)?;

        if samples.is_empty() || samples.iter().any(|sample| !sample.is_finite()) {
            return Err(Error::Decode(
                "WMA decoder returned no finite samples".to_string(),
            ));
        }
        if let Some(expected_samples) = expected_samples {
            if samples.len() < expected_samples {
                return Err(Error::Decode(format!(
                    "WMA decoder returned only {} samples for a {expected_samples}-sample packet table",
                    samples.len()
                )));
            }
            samples.truncate(expected_samples);
        }
        Ok(Pcm {
            sample_rate: parsed.format.sample_rate,
            channels: parsed.format.channels,
            samples,
        })
    }
}

fn normalized_xwma_bitrate(xwma: &Xwma<'_>) -> u32 {
    let format = &xwma.format;
    let mut bit_rate = format.avg_bytes_per_sec.saturating_mul(8);
    match (format.channels, format.sample_rate, bit_rate) {
        (1, 22_050 | 32_000, 48_000 | 192_000) => bit_rate = 20_000,
        (1, 44_100, 96_000 | 192_000) | (2, 32_000, 192_000) => bit_rate = 48_000,
        (2, 22_050, 48_000 | 192_000) => bit_rate = 32_000,
        _ => {}
    }
    bit_rate
}

fn drain_frames(
    decoder: &mut dyn oxideav_core::Decoder,
    output: &mut Vec<f32>,
    after_flush: bool,
) -> Result<()> {
    use oxideav_core::{Error as CodecError, Frame};

    loop {
        match decoder.receive_frame() {
            Ok(Frame::Audio(frame)) => {
                let Some(data) = frame.data.first() else {
                    return Err(Error::Decode(
                        "WMA decoder returned an empty audio frame".to_string(),
                    ));
                };
                let (whole, tail) = data.as_chunks::<4>();
                if !tail.is_empty() {
                    return Err(Error::Decode(
                        "WMA decoder returned a partial float sample".to_string(),
                    ));
                }
                output.extend(whole.iter().map(|sample| f32::from_le_bytes(*sample)));
                if output.len() > 256 * 1024 * 1024 / 4 {
                    return Err(Error::Decode("WMA decoded PCM exceeds 256 MiB".to_string()));
                }
            }
            Ok(Frame::Video(_)) => {
                return Err(Error::Decode(
                    "WMA decoder returned a video frame".to_string(),
                ));
            }
            Ok(_) => {
                return Err(Error::Decode(
                    "WMA decoder returned an unknown frame".to_string(),
                ));
            }
            Err(CodecError::NeedMore) if !after_flush => return Ok(()),
            Err(CodecError::Eof) if after_flush => return Ok(()),
            Err(error) => return Err(Error::Decode(format!("WMA output: {error}"))),
        }
    }
}

/// A decoder that runs the `ffmpeg` binary, writing the file to its standard
/// input and reading raw 32-bit float samples back.
#[cfg(feature = "xwma-ffmpeg")]
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    /// The program to run, `ffmpeg` unless a caller points somewhere else.
    pub program: String,
}

#[cfg(feature = "xwma-ffmpeg")]
impl Default for Ffmpeg {
    fn default() -> Self {
        Self {
            program: "ffmpeg".to_string(),
        }
    }
}

#[cfg(feature = "xwma-ffmpeg")]
impl Ffmpeg {
    /// A decoder that runs `ffmpeg` from the path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the program answers `-version`, so a caller can report a
    /// missing ffmpeg once rather than once per file.
    #[must_use]
    pub fn available(&self) -> bool {
        std::process::Command::new(&self.program)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

#[cfg(feature = "xwma-ffmpeg")]
impl WmaDecoder for Ffmpeg {
    fn decode_xwma(&self, file: &[u8]) -> Result<Pcm> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        // Read the header ourselves so the sample rate and channel count come
        // from the file rather than from parsing ffmpeg's chatter.
        let parsed = Xwma::parse(file)?;
        let sample_rate = parsed.format.sample_rate;
        let channels = parsed.format.channels;

        let mut child = Command::new(&self.program)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "xwma",
                "-i",
                "pipe:0",
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "pipe:1",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Decode(format!("could not run {}: {e}", self.program)))?;

        if let Some(mut stdin) = child.stdin.take() {
            // A decoder is free to stop reading early, and the broken pipe
            // that follows is not a decode failure.
            let _ = stdin.write_all(file);
        }
        let out = child
            .wait_with_output()
            .map_err(|e| Error::Decode(format!("{} failed: {e}", self.program)))?;
        if !out.status.success() {
            return Err(Error::Decode(format!(
                "{} exited with {}: {}",
                self.program,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }

        let (whole, _tail) = out.stdout.as_chunks::<4>();
        let samples = whole.iter().copied().map(f32::from_le_bytes).collect();
        Ok(Pcm {
            sample_rate,
            channels,
            samples,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_with, normalized_xwma_bitrate, Native, WmaDecoder};
    use crate::error::{Error, Result};
    use crate::pcm::Pcm;
    use crate::xwma::{WaveFormat, Xwma};

    struct Stub;
    impl WmaDecoder for Stub {
        fn decode_xwma(&self, file: &[u8]) -> Result<Pcm> {
            // Prove the seam hands over a complete, parseable file.
            let x = Xwma::parse(file)?;
            Ok(Pcm {
                sample_rate: x.format.sample_rate,
                channels: x.format.channels,
                samples: vec![0.25; 4],
            })
        }
    }

    struct Broken;
    impl WmaDecoder for Broken {
        fn decode_xwma(&self, _file: &[u8]) -> Result<Pcm> {
            Err(Error::Decode("no decoder".to_string()))
        }
    }

    fn built() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"XWMA");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&18u32.to_le_bytes());
        body.extend_from_slice(&0x0161u16.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&44100u32.to_le_bytes());
        body.extend_from_slice(&4000u32.to_le_bytes());
        body.extend_from_slice(&1487u16.to_le_bytes());
        body.extend_from_slice(&16u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(b"data");
        body.extend_from_slice(&2u32.to_le_bytes());
        body.extend_from_slice(&[1, 2]);
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn the_seam_hands_a_whole_file_to_the_decoder() {
        let bytes = built();
        let x = Xwma::parse(&bytes).unwrap();
        let pcm = decode_with(&Stub, &x).unwrap();
        assert_eq!(pcm.sample_rate, 44100);
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.frames(), 4);
    }

    #[test]
    fn a_decoder_that_is_not_there_reports_rather_than_panics() {
        let bytes = built();
        let x = Xwma::parse(&bytes).unwrap();
        assert!(matches!(decode_with(&Broken, &x), Err(Error::Decode(_))));
    }

    #[test]
    fn aud_02_rejects_xwma_data_that_breaks_packet_alignment() {
        let error = Native.decode_xwma(&built()).unwrap_err();
        assert!(error.to_string().contains("not a nonzero multiple"));
    }

    #[test]
    fn aud_02_normalizes_xwma_bitrate_aliases() {
        let cases = [
            (1, 22_050, 48_000, 20_000),
            (1, 32_000, 192_000, 20_000),
            (1, 44_100, 96_000, 48_000),
            (2, 22_050, 192_000, 32_000),
            (2, 32_000, 192_000, 48_000),
            (2, 44_100, 192_000, 192_000),
        ];
        let got: Vec<u32> = cases
            .iter()
            .map(|&(channels, sample_rate, bit_rate, _)| {
                let xwma = Xwma {
                    format: WaveFormat {
                        format_tag: 0x0161,
                        channels,
                        sample_rate,
                        avg_bytes_per_sec: bit_rate / 8,
                        block_align: 2230,
                        bits_per_sample: 16,
                        extra: Vec::new(),
                    },
                    packet_totals: Vec::new(),
                    data: &[],
                };
                normalized_xwma_bitrate(&xwma)
            })
            .collect();
        let want: Vec<u32> = cases.iter().map(|case| case.3).collect();
        assert_eq!(got, want);
    }
}
