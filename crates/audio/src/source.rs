// SPDX-License-Identifier: GPL-3.0-or-later

//! One entry point that turns a data path into samples, whichever of the
//! containers the file turns out to be.
//!
//! This is where the engine's rule lands in code: a vanilla line in a FUZ and
//! a mod's Ogg file resolve through the same load order and decode into the
//! same [`Pcm`], so shipping the open format costs a mod author nothing.

use crate::error::{Error, Result, Unsupported};
use crate::fuz::Fuz;
use crate::pcm::Pcm;
use crate::wma::WmaDecoder;
use crate::xwma::Xwma;
use soe_bsa::Vfs;

/// What a file turned out to be, decided by its bytes rather than its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    /// A FUZ wrapping a LIP track and an audio stream.
    Fuz,
    /// A bare xWMA, what a loose `.xwm` holds.
    Xwma,
    /// A RIFF WAVE.
    Wave,
    /// An Ogg carrying Vorbis.
    OggVorbis,
    /// An Ogg carrying Opus.
    OggOpus,
    /// The Switch port's Skyrim wrapper carrying standard NX Opus.
    NxOpus,
    /// Nothing this crate reads.
    Unknown,
}

/// The extensions the engine tries for one sound, in the order the game does.
pub const EXTENSIONS: [&str; 3] = ["fuz", "xwm", "wav"];

/// Decides a container from the first bytes of a file.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Container {
    if bytes.len() >= 4 && &bytes[..4] == b"FUZE" {
        return Container::Fuz;
    }
    if bytes.starts_with(&crate::fuz::NX_OPUS_SIGNATURE) {
        return Container::NxOpus;
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" {
        return match &bytes[8..12] {
            b"XWMA" => Container::Xwma,
            b"WAVE" => Container::Wave,
            _ => Container::Unknown,
        };
    }
    if bytes.len() >= 4 && &bytes[..4] == b"OggS" {
        // The identification packet sits in the first page, so a bounded
        // window over the head is enough to tell the two codecs apart.
        let head = &bytes[..bytes.len().min(256)];
        if find(head, b"OpusHead").is_some() {
            return Container::OggOpus;
        }
        if find(head, b"vorbis").is_some() {
            return Container::OggVorbis;
        }
    }
    Container::Unknown
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A decoded sound, with the mouth track when the file carried one.
#[derive(Debug, Clone)]
pub struct Sound {
    pub container: Container,
    pub pcm: Pcm,
    /// The LIP block verbatim, present only for a FUZ that carried one. See
    /// [`crate::lip`] for what is and is not known about its contents.
    pub lip: Option<Vec<u8>>,
}

/// Reads sounds out of a load order, decoding whatever it finds.
///
/// The WMA decoder is optional on purpose: without one every other format
/// still plays and an xWMA reports [`Unsupported::Wma`] rather than failing
/// the whole build.
pub struct SoundSource<'a> {
    vfs: &'a Vfs,
    wma: Option<&'a dyn WmaDecoder>,
}

impl<'a> SoundSource<'a> {
    /// A source over one load order, with no WMA decoder.
    #[must_use]
    pub fn new(vfs: &'a Vfs) -> Self {
        Self { vfs, wma: None }
    }

    /// Adds the decoder that handles vanilla voice lines.
    #[must_use]
    pub fn with_wma_decoder(mut self, decoder: &'a dyn WmaDecoder) -> Self {
        self.wma = Some(decoder);
        self
    }

    /// Resolves a data path the way the game does, trying the sound
    /// extensions in turn when the path carries none.
    #[must_use]
    pub fn resolve(&self, data_path: &str) -> Option<String> {
        let path = soe_bsa::normalize(data_path);
        if self.vfs.exists(&path) {
            return Some(path);
        }
        let stem = path.rsplit_once('.').map_or(path.as_str(), |(s, _)| s);
        EXTENSIONS
            .iter()
            .map(|ext| format!("{stem}.{ext}"))
            .find(|candidate| self.vfs.exists(candidate))
    }

    /// Reads and decodes one sound by data path.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when nothing in the load order answers the path, and
    /// whatever [`SoundSource::decode`] reports for the bytes.
    pub fn read(&self, data_path: &str) -> Result<Sound> {
        let found = self.resolve(data_path).ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no sound at {data_path}"),
            ))
        })?;
        let bytes = self
            .vfs
            .read(&found)
            .map_err(|e| Error::Decode(format!("{found}: {e}")))?;
        self.decode(&bytes)
    }

    /// Decodes bytes already in hand.
    ///
    /// # Errors
    ///
    /// [`Error::Unsupported`] when the container needs a decoder this build
    /// does not have, and the decoder's own error otherwise.
    pub fn decode(&self, bytes: &[u8]) -> Result<Sound> {
        let container = sniff(bytes);
        match container {
            Container::Fuz => {
                let fuz = Fuz::split(bytes)?;
                let lip = fuz.has_lip().then(|| fuz.lip.to_vec());
                let mut inner = self.decode(fuz.audio)?;
                inner.container = Container::Fuz;
                inner.lip = lip;
                Ok(inner)
            }
            Container::Xwma => {
                let parsed = Xwma::parse(bytes)?;
                let decoder = self.wma.ok_or(Error::Unsupported(Unsupported::Wma {
                    format_tag: parsed.format.format_tag,
                }))?;
                Ok(Sound {
                    container,
                    pcm: crate::wma::decode_with(decoder, &parsed)?,
                    lip: None,
                })
            }
            Container::Wave => Ok(Sound {
                container,
                pcm: decode_wave(bytes)?,
                lip: None,
            }),
            Container::OggVorbis => Ok(Sound {
                container,
                pcm: decode_vorbis(bytes)?,
                lip: None,
            }),
            Container::OggOpus => Ok(Sound {
                container,
                pcm: decode_opus(bytes)?,
                lip: None,
            }),
            Container::NxOpus => Ok(Sound {
                container,
                pcm: decode_nx_opus(bytes)?,
                lip: None,
            }),
            Container::Unknown => Err(Error::Unsupported(Unsupported::Extension(
                "unrecognised container".to_string(),
            ))),
        }
    }
}

/// Decodes a RIFF WAVE through `hound`.
///
/// # Errors
///
/// [`Error::Decode`] when the file is not a WAV this build reads.
pub fn decode_wave(bytes: &[u8]) -> Result<Pcm> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Decode(format!("wav: {e}")))?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Decode(format!("wav samples: {e}")))?,
        hound::SampleFormat::Int => {
            // Full scale for a signed sample of this width, so 16 bit maps
            // to 32,768 and the result lands in -1.0 to 1.0.
            let scale = f32::from(spec.bits_per_sample.saturating_sub(1)).exp2();
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / scale))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::Decode(format!("wav samples: {e}")))?
        }
    };
    Ok(Pcm {
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

#[cfg(feature = "vorbis")]
fn decode_vorbis(bytes: &[u8]) -> Result<Pcm> {
    crate::vorbis::decode(bytes)
}

#[cfg(not(feature = "vorbis"))]
fn decode_vorbis(_bytes: &[u8]) -> Result<Pcm> {
    Err(Error::Unsupported(Unsupported::Vorbis))
}

#[cfg(feature = "opus")]
fn decode_opus(bytes: &[u8]) -> Result<Pcm> {
    crate::opus::decode(bytes)
}

#[cfg(not(feature = "opus"))]
fn decode_opus(_bytes: &[u8]) -> Result<Pcm> {
    Err(Error::Unsupported(Unsupported::Opus))
}

#[cfg(feature = "opus")]
fn decode_nx_opus(bytes: &[u8]) -> Result<Pcm> {
    crate::nx_opus::decode(bytes)
}

#[cfg(not(feature = "opus"))]
fn decode_nx_opus(_bytes: &[u8]) -> Result<Pcm> {
    Err(Error::Unsupported(Unsupported::Opus))
}

#[cfg(test)]
mod tests {
    use super::{decode_wave, sniff, Container, SoundSource};
    use crate::error::{Error, Unsupported};
    use crate::fuz;
    use soe_bsa::Vfs;

    fn wav(samples: &[i16]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut out, spec).unwrap();
            for s in samples {
                w.write_sample(*s).unwrap();
            }
            w.finalize().unwrap();
        }
        out.into_inner()
    }

    #[test]
    fn every_container_is_told_apart_by_its_bytes() {
        assert_eq!(sniff(&fuz::write(1, &[], b"RIFF")), Container::Fuz);
        assert_eq!(sniff(&wav(&[0, 1])), Container::Wave);
        assert_eq!(sniff(b"OggS\x00\x02OpusHead...."), Container::OggOpus);
        assert_eq!(sniff(b"OggS\x00\x02\x01vorbis...."), Container::OggVorbis);
        assert_eq!(sniff(b"nope"), Container::Unknown);
        assert_eq!(sniff(&[]), Container::Unknown);
        assert_eq!(sniff(&fuz::NX_OPUS_SIGNATURE), Container::NxOpus);
    }

    #[cfg(not(feature = "opus"))]
    #[test]
    fn nx_opus_reports_the_disabled_codec_in_a_fuz() {
        let vfs = Vfs::new();
        let bytes = fuz::write(1, &[], &fuz::NX_OPUS_SIGNATURE);
        assert!(matches!(
            SoundSource::new(&vfs).decode(&bytes),
            Err(Error::Unsupported(Unsupported::Opus))
        ));
    }

    #[test]
    fn a_wav_decodes_to_the_samples_it_was_written_with() {
        let bytes = wav(&[0, 16384, -16384, 0]);
        let pcm = decode_wave(&bytes).unwrap();
        assert_eq!(pcm.sample_rate, 22050);
        assert_eq!(pcm.channels, 1);
        assert_eq!(pcm.frames(), 4);
        assert!((pcm.peak() - 0.5).abs() < 0.01, "peak {}", pcm.peak());
    }

    #[test]
    fn a_fuz_wrapping_a_wav_keeps_its_lip_and_reports_as_a_fuz() {
        let vfs = Vfs::new();
        let src = SoundSource::new(&vfs);
        let bytes = fuz::write(1, &[9, 9, 9], &wav(&[0, 1000]));
        let sound = src.decode(&bytes).unwrap();
        assert_eq!(sound.container, Container::Fuz);
        assert_eq!(sound.lip.as_deref(), Some(&[9u8, 9, 9][..]));
        assert_eq!(sound.pcm.frames(), 2);
    }

    #[test]
    fn an_xwma_without_a_decoder_says_so_rather_than_failing_oddly() {
        let vfs = Vfs::new();
        let src = SoundSource::new(&vfs);
        let mut body = Vec::new();
        body.extend_from_slice(b"XWMA");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&0x0161u16.to_le_bytes());
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&44100u32.to_le_bytes());
        body.extend_from_slice(&4000u32.to_le_bytes());
        body.extend_from_slice(&1487u16.to_le_bytes());
        body.extend_from_slice(&16u16.to_le_bytes());
        body.extend_from_slice(b"data");
        body.extend_from_slice(&0u32.to_le_bytes());
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&body);
        assert!(matches!(
            src.decode(&bytes),
            Err(Error::Unsupported(Unsupported::Wma { .. }))
        ));
    }

    #[test]
    fn an_empty_load_order_resolves_nothing_and_says_not_found() {
        let vfs = Vfs::new();
        let src = SoundSource::new(&vfs);
        assert!(src.resolve("sound/voice/a/b/c.fuz").is_none());
        assert!(matches!(
            src.read("sound/voice/a/b/c.fuz"),
            Err(Error::Io(_))
        ));
    }
}
