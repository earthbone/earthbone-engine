// SPDX-License-Identifier: GPL-3.0-or-later

//! Bethesda INI files and the layering the game applies to them.
//!
//! The format is the plain one: `[Section]` headers, `key=value` lines, `;`
//! comments. Keys carry their type as a prefix letter (`bUseTAA`,
//! `iShadowMapResolution`, `fShadowDistance`, `sLanguage`, `uGridsToLoad`).
//!
//! The game reads several files for one setting and the later one wins:
//! `Skyrim_Default.ini` in the game folder, then `Skyrim.ini`,
//! `SkyrimCustom.ini` and `SkyrimPrefs.ini` from the settings folder, in that
//! order (STEP, Skyrim Configuration Settings). A plugin's own INI (`X.ini`
//! next to `X.esp`) applies on top of those when the plugin is active, with
//! two rules: a list value replaces the whole list, and settings the engine
//! reads at startup (the archive lists above all) cannot be changed by a
//! plugin INI (UESP, INI files).

use std::collections::{BTreeMap, BTreeSet};

/// One parsed file, sections and entries in file order, nothing merged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ini {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub entries: Vec<(String, String)>,
}

impl Ini {
    /// Parse the text of one file. Lines that are neither a section header
    /// nor `key=value` are skipped, so a damaged line loses one setting, not
    /// the file. Entries before the first header land in a section named "".
    pub fn parse(text: &str) -> Ini {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut ini = Ini::default();
        let mut current: Option<Section> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix('[') {
                let name = rest.split(']').next().unwrap_or("").trim().to_string();
                if let Some(done) = current.take() {
                    ini.sections.push(done);
                }
                current = Some(Section {
                    name,
                    entries: Vec::new(),
                });
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let section = current.get_or_insert_with(Section::default);
            section
                .entries
                .push((key.trim().to_string(), value.trim().to_string()));
        }
        if let Some(done) = current {
            ini.sections.push(done);
        }
        ini
    }

    /// The last value written for a key, since that is what the game keeps
    /// when a file repeats one.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .filter(|s| s.name.eq_ignore_ascii_case(section))
            .flat_map(|s| s.entries.iter())
            .filter(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
            .next_back()
    }

    /// Render back to text, CRLF line ends as the game's own writer uses.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for s in &self.sections {
            if !s.name.is_empty() {
                out.push_str(&format!("[{}]\r\n", s.name));
            }
            for (k, v) in &s.entries {
                out.push_str(&format!("{k}={v}\r\n"));
            }
            out.push_str("\r\n");
        }
        out
    }
}

/// Where a value came from. Ranked: a higher layer wins over a lower one
/// whatever order the files were applied in, and two files of the same layer
/// resolve to the later one applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    /// `Skyrim_Default.ini` in the game folder.
    Default,
    /// `Skyrim.ini`.
    Base,
    /// `SkyrimCustom.ini`.
    Custom,
    /// `SkyrimPrefs.ini`.
    Prefs,
    /// `<plugin>.ini`, named by the plugin file.
    Plugin(String),
}

impl Layer {
    fn rank(&self) -> u8 {
        match self {
            Layer::Default => 0,
            Layer::Base => 1,
            Layer::Custom => 2,
            Layer::Prefs => 3,
            Layer::Plugin(_) => 4,
        }
    }
}

/// The type a key's prefix letter announces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    Int,
    UInt,
    Float,
    Text,
    Unknown,
}

pub fn kind_of(key: &str) -> SettingKind {
    match key.chars().next() {
        Some('b') => SettingKind::Bool,
        Some('i') => SettingKind::Int,
        Some('u') => SettingKind::UInt,
        Some('f') => SettingKind::Float,
        Some('s') => SettingKind::Text,
        _ => SettingKind::Unknown,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub value: String,
    pub layer: Layer,
}

/// A plugin INI tried to change a startup setting and was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub plugin: String,
    pub section: String,
    pub key: String,
}

/// The merged view of every INI layer.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    values: BTreeMap<(String, String), Value>,
    startup_only: BTreeSet<(String, String)>,
    pub refused: Vec<Refused>,
}

fn norm(section: &str, key: &str) -> (String, String) {
    (section.to_ascii_lowercase(), key.to_ascii_lowercase())
}

/// Settings the engine reads once at startup, so a plugin INI cannot change
/// them. The archive keys and the test-file keys are the documented ones
/// (STEP, UESP), the grass and cell keys are the ones STEP names as examples.
pub fn default_startup_only() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Archive", "sResourceArchiveList"),
        ("Archive", "sResourceArchiveList2"),
        ("Archive", "sResourceStartUpArchiveList"),
        ("Archive", "sResourceArchiveMemoryCacheList"),
        ("Archive", "sArchiveToLoadInMemoryList"),
        ("Archive", "sResourceDataDirsFinal"),
        ("Archive", "bLoadArchiveInMemory"),
        ("Archive", "bInvalidateOlderFiles"),
        ("General", "sLanguage"),
        ("General", "sTestFile1"),
        ("General", "sTestFile2"),
        ("General", "sTestFile3"),
        ("General", "sTestFile4"),
        ("General", "sTestFile5"),
        ("General", "sTestFile6"),
        ("General", "sTestFile7"),
        ("General", "sTestFile8"),
        ("General", "sTestFile9"),
        ("General", "sTestFile10"),
        ("General", "uGridsToLoad"),
        ("General", "uExterior Cell Buffer"),
        ("Grass", "iMinGrassSize"),
        ("Grass", "fGrassStartFadeDistance"),
        ("Grass", "fGrassMaxStartFadeDistance"),
        ("Grass", "fGrassMinStartFadeDistance"),
    ]
}

impl Settings {
    pub fn new() -> Settings {
        let mut s = Settings::default();
        for (section, key) in default_startup_only() {
            s.startup_only.insert(norm(section, key));
        }
        s
    }

    /// Replace the startup-only set, for a game or a build that reads a
    /// different set at startup.
    pub fn with_startup_only<'a>(
        mut self,
        keys: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Settings {
        self.startup_only = keys.into_iter().map(|(s, k)| norm(s, k)).collect();
        self
    }

    pub fn is_startup_only(&self, section: &str, key: &str) -> bool {
        self.startup_only.contains(&norm(section, key))
    }

    /// Lay one file over the current view.
    pub fn apply(&mut self, layer: Layer, ini: &Ini) {
        for section in &ini.sections {
            for (key, value) in &section.entries {
                let id = norm(&section.name, key);
                if let Layer::Plugin(plugin) = &layer {
                    if self.startup_only.contains(&id) {
                        self.refused.push(Refused {
                            plugin: plugin.clone(),
                            section: section.name.clone(),
                            key: key.clone(),
                        });
                        continue;
                    }
                }
                if let Some(existing) = self.values.get(&id) {
                    if existing.layer.rank() > layer.rank() {
                        continue;
                    }
                }
                self.values.insert(
                    id,
                    Value {
                        value: value.clone(),
                        layer: layer.clone(),
                    },
                );
            }
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.values.get(&norm(section, key)).map(|v| v.value.as_str())
    }

    pub fn source(&self, section: &str, key: &str) -> Option<&Layer> {
        self.values.get(&norm(section, key)).map(|v| &v.layer)
    }

    /// `1` and `true` are on, `0` and `false` are off, anything else is unset.
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        match self.get(section, key)?.trim() {
            "1" | "true" | "True" | "TRUE" => Some(true),
            "0" | "false" | "False" | "FALSE" => Some(false),
            _ => None,
        }
    }

    pub fn get_int(&self, section: &str, key: &str) -> Option<i64> {
        self.get(section, key)?.trim().parse().ok()
    }

    pub fn get_uint(&self, section: &str, key: &str) -> Option<u64> {
        self.get(section, key)?.trim().parse().ok()
    }

    pub fn get_float(&self, section: &str, key: &str) -> Option<f64> {
        self.get(section, key)?.trim().parse().ok()
    }

    /// A comma-separated list, entries trimmed and empty ones dropped.
    pub fn get_list(&self, section: &str, key: &str) -> Vec<String> {
        self.get(section, key)
            .map(split_list)
            .unwrap_or_default()
    }

    /// The archives the game opens before anything else, first list then
    /// the second, in the order written.
    pub fn archives(&self) -> Vec<String> {
        let mut out = self.get_list("Archive", "sResourceArchiveList");
        out.extend(self.get_list("Archive", "sResourceArchiveList2"));
        out
    }

    /// Every setting, section and key lowercased, for diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = (&(String, String), &Value)> {
        self.values.iter()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

pub fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections_comments_bom_and_crlf() {
        let text = "\u{feff}; a comment\r\n[General]\r\nsLanguage=ENGLISH\r\nuGridsToLoad = 5 \r\n# hash comment\r\n\r\n[Display]\r\nbUseTAA=1\r\nbroken line\r\nbUseTAA=0\r\n";
        let ini = Ini::parse(text);
        assert_eq!(ini.sections.len(), 2);
        assert_eq!(ini.get("general", "slanguage"), Some("ENGLISH"));
        assert_eq!(ini.get("General", "uGridsToLoad"), Some("5"));
        assert_eq!(ini.get("Display", "bUseTAA"), Some("0"), "last wins");
        assert_eq!(ini.sections[1].entries.len(), 2, "the broken line is dropped");
    }

    #[test]
    fn entries_before_any_header_keep_a_blank_section() {
        let ini = Ini::parse("x=1\n[A]\ny=2\n");
        assert_eq!(ini.sections[0].name, "");
        assert_eq!(ini.get("", "x"), Some("1"));
        let back = Ini::parse(&ini.render());
        assert_eq!(back, ini);
    }

    #[test]
    fn layers_rank_whatever_order_they_are_applied() {
        let mut s = Settings::new();
        s.apply(Layer::Prefs, &Ini::parse("[Display]\nbUseTAA=0\n"));
        s.apply(Layer::Base, &Ini::parse("[Display]\nbUseTAA=1\nfShadowDistance=3000\n"));
        s.apply(Layer::Default, &Ini::parse("[Display]\nbUseTAA=1\nfShadowDistance=1000\niX=2\n"));
        s.apply(Layer::Custom, &Ini::parse("[Display]\nfShadowDistance=8000\n"));
        assert_eq!(s.get_bool("Display", "bUseTAA"), Some(false));
        assert_eq!(s.source("Display", "bUseTAA"), Some(&Layer::Prefs));
        assert_eq!(s.get_float("Display", "fShadowDistance"), Some(8000.0));
        assert_eq!(s.get_int("Display", "iX"), Some(2));
        assert_eq!(s.source("display", "ix"), Some(&Layer::Default));
    }

    #[test]
    fn plugin_inis_win_over_prefs_except_for_startup_settings() {
        let mut s = Settings::new();
        s.apply(
            Layer::Default,
            &Ini::parse("[Archive]\nsResourceArchiveList=A.bsa, B.bsa\n[Grass]\nfGrassFadeRange=3000\n"),
        );
        s.apply(Layer::Prefs, &Ini::parse("[Grass]\nfGrassFadeRange=5000\n"));
        s.apply(
            Layer::Plugin("Mod.esp".into()),
            &Ini::parse("[Archive]\nsResourceArchiveList=Evil.bsa\n[Grass]\nfGrassFadeRange=7000\n"),
        );
        assert_eq!(s.archives(), vec!["A.bsa", "B.bsa"]);
        assert_eq!(s.get_float("Grass", "fGrassFadeRange"), Some(7000.0));
        assert_eq!(s.refused.len(), 1);
        assert_eq!(s.refused[0].plugin, "Mod.esp");
        assert_eq!(s.refused[0].key, "sResourceArchiveList");
    }

    #[test]
    fn a_later_plugin_ini_replaces_a_list_wholesale() {
        let mut s = Settings::new().with_startup_only(Vec::<(&str, &str)>::new());
        s.apply(Layer::Base, &Ini::parse("[Archive]\nsResourceArchiveList=A.bsa, B.bsa\n"));
        s.apply(Layer::Plugin("One.esp".into()), &Ini::parse("[Archive]\nsResourceArchiveList=C.bsa\n"));
        s.apply(Layer::Plugin("Two.esp".into()), &Ini::parse("[Archive]\nsResourceArchiveList=D.bsa,,E.bsa\n"));
        assert_eq!(s.archives(), vec!["D.bsa", "E.bsa"]);
        assert_eq!(s.source("Archive", "sResourceArchiveList"), Some(&Layer::Plugin("Two.esp".into())));
    }

    #[test]
    fn typed_getters_follow_the_prefix_rules() {
        let mut s = Settings::new();
        s.apply(Layer::Base, &Ini::parse("[A]\nbOn=true\nbOff=0\nbOdd=maybe\niN=-3\nuN=7\nfN=1.5\nsT=hello\n"));
        assert_eq!(s.get_bool("A", "bOn"), Some(true));
        assert_eq!(s.get_bool("A", "bOff"), Some(false));
        assert_eq!(s.get_bool("A", "bOdd"), None);
        assert_eq!(s.get_int("A", "iN"), Some(-3));
        assert_eq!(s.get_uint("A", "iN"), None);
        assert_eq!(s.get_uint("A", "uN"), Some(7));
        assert_eq!(s.get_float("A", "fN"), Some(1.5));
        assert_eq!(s.get("A", "sT"), Some("hello"));
        assert_eq!(kind_of("bOn"), SettingKind::Bool);
        assert_eq!(kind_of("uN"), SettingKind::UInt);
        assert_eq!(kind_of("Weird"), SettingKind::Unknown);
        assert_eq!(s.len(), 7);
    }
}
