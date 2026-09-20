// SPDX-License-Identifier: GPL-3.0-or-later

//! Locale selection for Skyrim voice archives.
//!
//! `sLanguage` chooses the voice archive suffix. The archive list still owns
//! ordering, this module only keeps another locale's voice files out of that
//! list. Skyrim-NX-Toolkit's shipped `Skyrim_fr.ini` and `Skyrim_de.ini` list
//! `Voices_fr0` and `Voices_de0` respectively.

use std::path::Path;

/// A locale represented by the suffix in a `Voices_<locale><part>.bsa` name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceLocale(String);

/// An invalid or unsupported value from `[General] sLanguage`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VoiceArchiveError {
    #[error("[General] sLanguage is missing")]
    MissingLanguage,
    #[error("unsupported [General] sLanguage '{0}'")]
    UnsupportedLanguage(String),
}

impl VoiceLocale {
    /// Converts Skyrim's `sLanguage` value into its voice archive suffix.
    pub fn from_language(language: Option<&str>) -> Result<Self, VoiceArchiveError> {
        let language = language.ok_or(VoiceArchiveError::MissingLanguage)?;
        let code = match language.trim().to_ascii_uppercase().as_str() {
            "ENGLISH" => "en",
            "FRENCH" => "fr",
            "GERMAN" => "de",
            "ITALIAN" => "it",
            "JAPANESE" => "ja",
            "RUSSIAN" => "ru",
            "SPANISH" => "es",
            _ => return Err(VoiceArchiveError::UnsupportedLanguage(language.to_owned())),
        };
        Ok(Self(code.to_owned()))
    }

    /// Whether `name` is a voice archive for this locale.
    #[must_use]
    pub fn matches_archive(&self, name: &str) -> bool {
        archive_locale(name).is_some_and(|locale| locale == self.0)
    }

    /// Whether `name` is a voice archive for any locale.
    #[must_use]
    pub fn is_voice_archive(name: &str) -> bool {
        archive_locale(name).is_some()
    }
}

fn archive_locale(name: &str) -> Option<String> {
    let name = name.to_ascii_lowercase();
    let path = Path::new(&name);
    if !path.extension()?.eq_ignore_ascii_case("bsa") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let (_, suffix) = stem.rsplit_once(" - voices_")?;
    let locale = suffix.trim_end_matches(|c: char| c.is_ascii_digit());
    if locale.len() != 2 || !locale.bytes().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some(locale.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{VoiceArchiveError, VoiceLocale};

    #[test]
    fn a_french_selection_excludes_competing_voice_locales() {
        let french = VoiceLocale::from_language(Some("FRENCH")).unwrap();
        assert!(french.matches_archive("Skyrim - Voices_fr0.bsa"));
        assert!(french.matches_archive("Dawnguard - Voices_fr1.bsa"));
        assert!(!french.matches_archive("Skyrim - Voices_en0.bsa"));
        assert!(VoiceLocale::is_voice_archive("Skyrim - Voices_en0.bsa"));
        assert!(french.matches_archive("SKYRIM - VOICES_FR0.BSA"));
        assert!(!french.matches_archive("Skyrim - Voices_EN0.bsa"));
    }

    #[test]
    fn unavailable_or_unknown_locale_has_no_english_fallback() {
        assert_eq!(
            VoiceLocale::from_language(None),
            Err(VoiceArchiveError::MissingLanguage)
        );
        assert_eq!(
            VoiceLocale::from_language(Some("KLINGON")),
            Err(VoiceArchiveError::UnsupportedLanguage("KLINGON".into()))
        );
    }
}
