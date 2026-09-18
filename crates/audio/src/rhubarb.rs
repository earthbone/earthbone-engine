// SPDX-License-Identifier: GPL-3.0-or-later

//! Mouth shapes from audio, by running Rhubarb Lip Sync out of process.
//!
//! No recogniser is written here and none should be. Rhubarb Lip Sync is MIT
//! licensed, ships a Linux build, and already does the hard part: it listens
//! to a recording, optionally constrained by the line's written text, and
//! reports which of nine mouth positions to hold at each moment. Its own
//! licence notes that the data it produces belongs to whoever ran it, so
//! nothing it emits encumbers a generated track.
//!
//! What this module adds is the seam. [`Rhubarb`] implements
//! [`crate::viseme::VisemeSource`], so the engine depends on the idea of a
//! shape source rather than on this tool, and the whole thing sits behind a
//! feature that is off by default. Same trade as ffmpeg in [`crate::wma`]:
//! a subprocess costs one process per line and nothing else, where linking
//! would drag a C++ speech recogniser into a core crate and tie the build to
//! it. See `docs/audio.md`.
//!
//! Rhubarb reads a file rather than a stream, so a decoded [`Pcm`] is written
//! to a temporary 16-bit mono WAV first, through `hound`, which the crate
//! already carries.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};
use crate::pcm::Pcm;
use crate::viseme::{MouthCue, Shape, VisemeSource, VisemeTrack};

/// A temporary file that deletes itself, so a failed run does not leave the
/// system temp directory full of voice lines.
struct Scratch(PathBuf);

impl Scratch {
    fn new(extension: &str) -> Scratch {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let name = format!("soe-visemes-{}-{n}.{extension}", std::process::id());
        Scratch(std::env::temp_dir().join(name))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Losing a temp file is not worth failing a drop over.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Runs the `rhubarb` binary to derive mouth shapes from audio.
#[derive(Debug, Clone)]
pub struct Rhubarb {
    /// The program to run, `rhubarb` unless a caller points somewhere else.
    pub program: String,
    /// Seconds of crossfade the consumer should use at a cue boundary. Not
    /// passed to the tool, carried here so a caller has one place to set it.
    pub blend: f64,
}

impl Default for Rhubarb {
    fn default() -> Self {
        Self {
            program: "rhubarb".to_string(),
            // Rhubarb quantises cues to hundredths of a second, so four
            // hundredths is a visible crossfade that still lands inside the
            // shortest cues it emits.
            blend: 0.04,
        }
    }
}

impl Rhubarb {
    /// A source that runs `rhubarb` from the path.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A source that runs the binary at `program`.
    #[must_use]
    pub fn at(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    /// True when the program answers `--version`, so a caller can report a
    /// missing tool once rather than once per line.
    #[must_use]
    pub fn available(&self) -> bool {
        Command::new(&self.program)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Writes `pcm` as a 16-bit mono WAV, which is what a speech recogniser wants
/// and what Rhubarb documents as its input.
fn write_wav(pcm: &Pcm, path: &Path) -> Result<()> {
    if pcm.sample_rate == 0 || pcm.channels == 0 {
        return Err(Error::Decode(
            "cannot write a WAV for a stream with no rate or channels".to_string(),
        ));
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: pcm.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec)
        .map_err(|e| Error::Decode(format!("could not write {}: {e}", path.display())))?;
    let channels = pcm.channels as usize;
    for frame in pcm.samples.chunks(channels) {
        // Downmix, since the recogniser wants one channel and the vanilla
        // voice files are mono anyway.
        let mixed = frame.iter().sum::<f32>() / frame.len() as f32;
        let clamped = mixed.clamp(-1.0, 1.0);
        let sample = (clamped * f32::from(i16::MAX)) as i16;
        w.write_sample(sample)
            .map_err(|e| Error::Decode(format!("writing samples: {e}")))?;
    }
    w.finalize()
        .map_err(|e| Error::Decode(format!("closing {}: {e}", path.display())))
}

/// Reads Rhubarb's JSON into a track.
///
/// The shape is `{"metadata": {"duration": f}, "mouthCues": [{"start": f,
/// "end": f, "value": "A"}]}`, documented in the tool's own `README.adoc`.
fn parse_json(text: &str) -> Result<VisemeTrack> {
    let root: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::Decode(format!("rhubarb did not emit JSON: {e}")))?;
    let duration = root
        .get("metadata")
        .and_then(|m| m.get("duration"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    let cues = root
        .get("mouthCues")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| Error::Decode("rhubarb JSON has no mouthCues array".to_string()))?;
    let mut out = Vec::with_capacity(cues.len());
    for cue in cues {
        let start = cue.get("start").and_then(serde_json::Value::as_f64);
        let end = cue.get("end").and_then(serde_json::Value::as_f64);
        let value = cue.get("value").and_then(serde_json::Value::as_str);
        let (Some(start), Some(end), Some(value)) = (start, end, value) else {
            return Err(Error::Decode(format!("incomplete mouth cue: {cue}")));
        };
        let shape = value
            .chars()
            .next()
            .and_then(Shape::from_letter)
            .ok_or_else(|| Error::Decode(format!("unknown mouth shape {value:?}")))?;
        out.push(MouthCue { start, end, shape });
    }
    Ok(VisemeTrack {
        duration: if duration > 0.0 {
            duration
        } else {
            out.last().map_or(0.0, |c| c.end)
        },
        cues: out,
    })
}

impl VisemeSource for Rhubarb {
    fn visemes(&self, pcm: &Pcm, dialog: Option<&str>) -> Result<VisemeTrack> {
        let wav = Scratch::new("wav");
        write_wav(pcm, wav.path())?;

        let mut command = Command::new(&self.program);
        command.args(["--exportFormat", "json", "--quiet"]);

        // Held until the child exits, so the file outlives the run.
        let dialog_file = match dialog {
            None => None,
            Some(text) => {
                let scratch = Scratch::new("txt");
                let mut f = std::fs::File::create(scratch.path())
                    .map_err(|e| Error::Decode(format!("dialog file: {e}")))?;
                f.write_all(text.as_bytes())
                    .map_err(|e| Error::Decode(format!("dialog file: {e}")))?;
                command.arg("--dialogFile").arg(scratch.path());
                Some(scratch)
            }
        };

        let out = command
            .arg(wav.path())
            .output()
            .map_err(|e| Error::Decode(format!("could not run {}: {e}", self.program)))?;
        drop(dialog_file);
        if !out.status.success() {
            return Err(Error::Decode(format!(
                "{} exited with {}: {}",
                self.program,
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        parse_json(&String::from_utf8_lossy(&out.stdout))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_json, write_wav, Rhubarb};
    use crate::pcm::Pcm;
    use crate::viseme::Shape;

    #[test]
    fn the_documented_json_parses_into_a_track() {
        // The example from Rhubarb's own README, a person saying "Hi".
        let text = r#"{
          "metadata": { "soundFile": "hi.wav", "duration": 0.47 },
          "mouthCues": [
            { "start": 0.00, "end": 0.05, "value": "X" },
            { "start": 0.05, "end": 0.27, "value": "D" },
            { "start": 0.27, "end": 0.31, "value": "C" },
            { "start": 0.31, "end": 0.43, "value": "B" },
            { "start": 0.43, "end": 0.47, "value": "X" }
          ]
        }"#;
        let track = parse_json(text).unwrap();
        assert!((track.duration - 0.47).abs() < 1e-9);
        assert_eq!(track.cues.len(), 5);
        assert_eq!(track.shape_at(0.1), Shape::D);
        assert_eq!(track.shape_at(0.35), Shape::B);
    }

    #[test]
    fn a_missing_duration_falls_back_to_the_last_cue() {
        let text = r#"{"mouthCues":[{"start":0.0,"end":1.5,"value":"A"}]}"#;
        let track = parse_json(text).unwrap();
        assert!((track.duration - 1.5).abs() < 1e-9);
    }

    #[test]
    fn broken_output_is_reported_rather_than_guessed() {
        assert!(parse_json("not json at all").is_err());
        assert!(parse_json(r#"{"metadata":{}}"#).is_err());
        assert!(parse_json(r#"{"mouthCues":[{"start":0.0,"end":1.0}]}"#).is_err());
        assert!(parse_json(r#"{"mouthCues":[{"start":0.0,"end":1.0,"value":"Q"}]}"#).is_err());
    }

    #[test]
    fn a_stereo_buffer_downmixes_into_a_mono_wav() {
        let pcm = Pcm {
            sample_rate: 16000,
            channels: 2,
            samples: vec![1.0, -1.0, 0.5, 0.5],
        };
        let path = std::env::temp_dir().join(format!("soe-wav-test-{}.wav", std::process::id()));
        write_wav(&pcm, &path).unwrap();
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.spec().sample_rate, 16000);
        assert_eq!(reader.len(), 2);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_degenerate_buffer_is_refused_rather_than_written() {
        let pcm = Pcm::empty(0, 0);
        let path = std::env::temp_dir().join("soe-never-written.wav");
        assert!(write_wav(&pcm, &path).is_err());
    }

    #[test]
    fn a_missing_binary_reports_instead_of_panicking() {
        let r = Rhubarb::at("soe-no-such-program-anywhere");
        assert!(!r.available());
    }
}
