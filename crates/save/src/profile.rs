// SPDX-License-Identifier: GPL-3.0-or-later

//! Size and decode time of a save, per named section and per plugin.
//!
//! [`Save::parse_profiled`] runs the ordinary parse and records, for each
//! region of the uncompressed body, how many bytes it spans and how long it
//! took to decode. The plain [`Save::parse`] path takes no clock readings.

use std::collections::BTreeMap;
use std::time::Instant;

use crate::papyrus::Papyrus;
use crate::Save;

/// Size and decode time of one named region of a save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionProfile {
    pub name: &'static str,
    pub bytes: usize,
    pub nanos: u64,
}

/// What a profiled parse measured.
#[derive(Clone, Debug, Default)]
pub struct ParseProfile {
    /// Regions in file order. `container` is the uncompressed header block and
    /// its time includes inflating the body. The rest tile the body.
    pub sections: Vec<SectionProfile>,
    /// Decode time of each change form, parallel to `Save::change_forms`.
    pub change_form_nanos: Vec<u64>,
}

/// Change form decode work attributed to one plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginLoad {
    pub plugin: String,
    pub forms: usize,
    pub nanos: u64,
}

/// Plugin share of the form id array, four bytes per entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginFormIds {
    pub plugin: String,
    pub entries: usize,
    pub bytes: usize,
}

/// Papyrus variable data owned by one script on forms of one plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PapyrusScriptSize {
    pub script: String,
    pub plugin: String,
    pub instances: usize,
    pub bytes: usize,
}

/// A running clock that exists only when a profile was requested.
pub(crate) struct Lap(Option<(Instant, usize)>);

impl Lap {
    pub(crate) fn start(profile: &Option<&mut ParseProfile>, pos: usize) -> Self {
        Self(profile.is_some().then(|| (Instant::now(), pos)))
    }

    pub(crate) fn finish(
        self,
        profile: &mut Option<&mut ParseProfile>,
        name: &'static str,
        pos: usize,
    ) {
        if let (Some((started, from)), Some(profile)) = (self.0, profile.as_deref_mut()) {
            profile.sections.push(SectionProfile {
                name,
                bytes: pos.saturating_sub(from),
                nanos: u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
            });
        }
    }
}

pub(crate) fn nanos_since(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

impl Save {
    /// Change form decode time per plugin, from a profile of this save.
    #[must_use]
    pub fn plugin_load_nanos(&self, profile: &ParseProfile) -> Vec<PluginLoad> {
        let mut totals: BTreeMap<String, (usize, u64)> = BTreeMap::new();
        for (form, nanos) in self.change_forms.iter().zip(&profile.change_form_nanos) {
            let entry = totals.entry(self.plugin_of(form.form)).or_default();
            entry.0 += 1;
            entry.1 += nanos;
        }
        totals
            .into_iter()
            .map(|(plugin, (forms, nanos))| PluginLoad {
                plugin,
                forms,
                nanos,
            })
            .collect()
    }

    /// The form id array split by the plugin each entry names.
    #[must_use]
    pub fn form_id_array_by_plugin(&self) -> Vec<PluginFormIds> {
        let mut totals: BTreeMap<String, usize> = BTreeMap::new();
        for form_id in &self.form_id_array {
            let plugin = self
                .plugin_name_for_form_id(*form_id)
                .unwrap_or_else(|| "<unknown-plugin>".to_owned());
            *totals.entry(plugin).or_default() += 1;
        }
        totals
            .into_iter()
            .map(|(plugin, entries)| PluginFormIds {
                plugin,
                entries,
                bytes: entries * 4,
            })
            .collect()
    }

    /// Variable data bytes of every script instance, grouped by script and by
    /// the plugin of the form the instance is attached to. Reference and array
    /// data belong to no script instance and stay unattributed.
    pub fn papyrus_script_sizes(&self, papyrus: &Papyrus) -> crate::Result<Vec<PapyrusScriptSize>> {
        let data: BTreeMap<u64, usize> = papyrus
            .instance_data
            .iter()
            .map(|entry| entry.encoded_len().map(|len| (entry.id, len)))
            .collect::<crate::Result<_>>()?;
        let mut totals: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
        for instance in &papyrus.instances {
            let script = papyrus
                .string(instance.script)
                .unwrap_or("<unknown-script>")
                .to_owned();
            let plugin = self.plugin_of(instance.form);
            let entry = totals.entry((script, plugin)).or_default();
            entry.0 += 1;
            entry.1 += data.get(&instance.id).copied().unwrap_or(0);
        }
        Ok(totals
            .into_iter()
            .map(|((script, plugin), (instances, bytes))| PapyrusScriptSize {
                script,
                plugin,
                instances,
                bytes,
            })
            .collect())
    }
}
