// SPDX-License-Identifier: GPL-3.0-or-later

//! Decoding the WMA bitstream inside an xWMA file, through somebody else's
//! decoder.
//!
//! No WMA decoder is written here and none should be. ffmpeg has demuxed
//! xWMA and decoded `wmav2` for years, and its `xwma` demuxer is also the
//! reference for how the `dpds` packet table is meant to be read. What this
//! module adds is the seam: a [`WmaDecoder`] trait so the engine depends on
//! the idea of a decoder rather than on ffmpeg, and one implementation that
//! runs the `ffmpeg` binary out of process.
//!
//! Out of process, and behind a feature that is off by default, is a
//! deliberate trade. Linking ffmpeg through `ffmpeg-sys-next` needs the
//! development headers on every build machine (this one has the shared
//! libraries and no headers at all, so that route does not even compile
//! here), drags a large C dependency into a core crate, and ties the engine's
//! build to ffmpeg's ABI. Running the binary costs a process per file and
//! nothing else, and the container parsing in [`crate::xwma`] keeps working
//! with no decoder present at all. See `docs/audio.md` for the trade table.

#[cfg(feature = "xwma-ffmpeg")]
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
    use super::{decode_with, WmaDecoder};
    use crate::error::{Error, Result};
    use crate::pcm::Pcm;
    use crate::xwma::Xwma;

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
}
