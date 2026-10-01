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
    let duration = if duration > 0.0 {
        duration
    } else {
        out.last().map_or(0.0, |c| c.end)
    };
    if !duration.is_finite() || duration <= 0.0 {
        return Err(Error::Decode(
            "rhubarb track has invalid duration".to_string(),
        ));
    }
    if out.is_empty() || out[0].start != 0.0 || out.last().is_none_or(|cue| cue.end != duration) {
        return Err(Error::Decode(
            "rhubarb cues do not cover the complete track duration".to_string(),
        ));
    }
    if out.iter().enumerate().any(|(index, cue)| {
        !cue.start.is_finite()
            || !cue.end.is_finite()
            || cue.start < 0.0
            || cue.end <= cue.start
            || cue.end > duration
            || (index > 0 && out[index - 1].end != cue.start)
    }) {
        return Err(Error::Decode(
            "rhubarb cues are invalid or do not tile the track".to_string(),
        ));
    }
    Ok(VisemeTrack {
        duration,
        cues: out,
    })
}

impl Rhubarb {
    /// Returns the complete cache setting string for this generation request.
    ///
    /// The consumer blend does not appear here because it changes playback
    /// interpolation, not the track Rhubarb generates.
    ///
    /// # Errors
    ///
    /// Returns a decode error when Rhubarb cannot report its version.
    pub fn cache_settings(&self, pcm: &Pcm, dialog: Option<&str>) -> Result<String> {
        let version = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|error| Error::Decode(format!("could not run {}: {error}", self.program)))?;
        if !version.status.success() {
            return Err(Error::Decode(format!(
                "{} exited with {} while reporting its version",
                self.program, version.status
            )));
        }
        Ok(
            self.cache_settings_from_version(
                &String::from_utf8_lossy(&version.stdout),
                pcm,
                dialog,
            ),
        )
    }

    fn cache_settings_from_version(
        &self,
        version: &str,
        pcm: &Pcm,
        dialog: Option<&str>,
    ) -> String {
        let (dialog_present, dialog) = match dialog {
            Some(dialog) => (true, dialog),
            None => (false, ""),
        };
        let mut pcm_bytes = Vec::with_capacity(8 + pcm.samples.len() * 4);
        pcm_bytes.extend_from_slice(&pcm.sample_rate.to_le_bytes());
        pcm_bytes.extend_from_slice(&pcm.channels.to_le_bytes());
        for sample in &pcm.samples {
            pcm_bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
        format!(
            "rhubarb-cache-v2\nprogram:{}:{}\nversion:{}:{}\npcm:{:016x}\ndialog-present:{}\ndialog:{}:{}",
            self.program.len(),
            self.program,
            version.len(),
            version,
            crate::viseme::cache_hash(&pcm_bytes),
            usize::from(dialog_present),
            dialog.len(),
            dialog
        )
    }

    /// Returns a generated track from cache, or asks Rhubarb to generate it.
    ///
    /// Asset bytes work for loose files and archive entries. The cache identity
    /// includes their bytes, the decoded PCM, Rhubarb's reported version, the
    /// program selection and the dialogue.
    ///
    /// # Errors
    ///
    /// Returns an I/O error from the cache, or an error from Rhubarb.
    pub fn visemes_cached(
        &self,
        cache: &crate::viseme::VisemeCache,
        asset: &[u8],
        pcm: &Pcm,
        dialog: Option<&str>,
    ) -> Result<VisemeTrack> {
        let settings = self.cache_settings(pcm, dialog)?;
        cache.get_or_generate(asset, &settings, || self.visemes(pcm, dialog))
    }

    /// Generates mouth cues using the plain text of one INFO response as a
    /// recognition hint when that text is available.
    ///
    /// Localized string IDs have to be resolved by the caller before this
    /// method can use them. Missing or unresolved text falls back to audio-only
    /// recognition.
    ///
    /// # Errors
    ///
    /// Returns the same decode error as [`VisemeSource::visemes`] when the
    /// recognizer cannot run or its output is invalid.
    pub fn visemes_for_response(
        &self,
        pcm: &Pcm,
        response: &soe_esm::records::gameplay::Response,
    ) -> Result<VisemeTrack> {
        let dialog = response.text.as_ref().and_then(|text| text.plain());
        self.visemes(pcm, dialog)
    }
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

    static NEXT_TEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

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
    fn cache_settings_include_program_version_pcm_and_dialogue() {
        let source = Rhubarb::at("controlled-rhubarb");
        let pcm = Pcm {
            sample_rate: 16_000,
            channels: 1,
            samples: vec![0.0],
        };
        let no_dialog = source.cache_settings_from_version("1.14.0", &pcm, None);
        assert_ne!(
            no_dialog,
            source.cache_settings_from_version("1.14.0", &pcm, Some(""))
        );
        assert_ne!(
            source.cache_settings_from_version("1.14.0", &pcm, Some("first line")),
            source.cache_settings_from_version("1.14.0", &pcm, Some("second line"))
        );
        assert_ne!(
            source.cache_settings_from_version("1.14.0", &pcm, Some("first line")),
            source.cache_settings_from_version("1.15.0", &pcm, Some("first line"))
        );
        let changed_rate = Pcm {
            sample_rate: 44_100,
            ..pcm.clone()
        };
        assert_ne!(
            source.cache_settings_from_version("1.14.0", &pcm, Some("first line")),
            source.cache_settings_from_version("1.14.0", &changed_rate, Some("first line"))
        );
        let changed_channels = Pcm {
            channels: 2,
            ..pcm.clone()
        };
        assert_ne!(
            source.cache_settings_from_version("1.14.0", &pcm, Some("first line")),
            source.cache_settings_from_version("1.14.0", &changed_channels, Some("first line"))
        );
    }

    #[cfg(unix)]
    fn aud_10_fixture(text_path: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let program = text_path.with_extension("sh");
        let script = format!(
            "#!/bin/sh\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = \"--dialogFile\" ]; then\n    cat \"$2\" > '{}'\n    shift 2\n  else\n    shift\n  fi\ndone\ncat <<'JSON'\n{{\"metadata\":{{\"duration\":0.1}},\"mouthCues\":[{{\"start\":0.0,\"end\":0.1,\"value\":\"B\"}}]}}\nJSON\n",
            text_path.display()
        );
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }

    #[cfg(unix)]
    fn aud_10_response(
        text: Option<soe_esm::records::Text>,
    ) -> soe_esm::records::gameplay::Response {
        soe_esm::records::gameplay::Response {
            emotion: 0,
            emotion_value: 0,
            number: 1,
            sound: 0,
            use_emotion_animation: 0,
            text,
            speaker_idle: None,
            listener_idle: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn aud_10_plain_info_response_is_sent_as_the_rhubarb_hint() {
        let prefix = format!(
            "soe-aud-10-text-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let dialog_path = std::env::temp_dir().join(prefix);
        let program = aud_10_fixture(&dialog_path);
        let pcm = Pcm {
            sample_rate: 16_000,
            channels: 1,
            samples: vec![0.0; 1600],
        };
        let response = aud_10_response(Some(soe_esm::records::Text::Plain(
            "Speak clearly".to_string(),
        )));
        Rhubarb::at(program.to_string_lossy())
            .visemes_for_response(&pcm, &response)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&dialog_path).unwrap(),
            "Speak clearly"
        );
        std::fs::remove_file(program).unwrap();
        std::fs::remove_file(dialog_path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn aud_10_unresolved_info_text_runs_without_a_dialog_hint() {
        let prefix = format!(
            "soe-aud-10-empty-{}-{}",
            std::process::id(),
            NEXT_TEST.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let dialog_path = std::env::temp_dir().join(prefix);
        let program = aud_10_fixture(&dialog_path);
        let pcm = Pcm {
            sample_rate: 16_000,
            channels: 1,
            samples: vec![0.0; 1600],
        };
        let response = aud_10_response(Some(soe_esm::records::Text::Localized(0x1234)));
        Rhubarb::at(program.to_string_lossy())
            .visemes_for_response(&pcm, &response)
            .unwrap();
        assert!(!dialog_path.exists());
        std::fs::remove_file(program).unwrap();
    }

    #[test]
    fn a_missing_binary_reports_instead_of_panicking() {
        let r = Rhubarb::at("soe-no-such-program-anywhere");
        assert!(!r.available());
    }
}
