// SPDX-License-Identifier: GPL-3.0-or-later

//! Localized text resolution through the existing load-order aware VFS.

use std::collections::BTreeMap;
use std::path::Path;

use soe_bsa::Vfs;
use soe_esm::records::Text;
use soe_esm::strings::{StringTable, StringTableKind};
use soe_esm::Tag;

use super::World;

#[derive(Debug, thiserror::Error)]
pub enum LocalizationError {
    #[error("plugin name has no UTF-8 stem: {0}")]
    PluginName(String),
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: soe_bsa::Error,
    },
    #[error("could not parse {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: soe_esm::Error,
    },
}

impl World {
    /// Load every localized plugin's selected language through VFS precedence.
    ///
    /// Missing companion tables are normal because a plugin may contain only
    /// one kind of localized field.
    pub fn load_localized_strings(
        &mut self,
        vfs: &Vfs,
        language: &str,
    ) -> Result<usize, LocalizationError> {
        let mut strings = BTreeMap::new();
        let mut loaded = 0;
        for (file, plugin) in self.order.files().iter().enumerate() {
            if !plugin.plugin.header.is_localized() {
                continue;
            }
            let stem = Path::new(&plugin.name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or_else(|| LocalizationError::PluginName(plugin.name.clone()))?;
            for kind in [
                StringTableKind::Dl,
                StringTableKind::Il,
                StringTableKind::Plain,
            ] {
                let path = format!("strings/{stem}_{language}.{}", kind.extension());
                if !vfs.exists(&path) {
                    continue;
                }
                let bytes = vfs.read(&path).map_err(|source| LocalizationError::Read {
                    path: path.clone(),
                    source,
                })?;
                let table = StringTable::parse(&bytes, kind, language)
                    .map_err(|source| LocalizationError::Parse { path, source })?;
                strings.insert((file as u16, kind), table);
                loaded += 1;
            }
        }
        self.localized_strings = strings;
        Ok(loaded)
    }

    pub fn resolve_text(&self, id: soe_esm::GlobalFormId, field: Tag, text: &Text) -> String {
        match text {
            Text::Plain(text) => text.clone(),
            Text::Localized(string_id) => self
                .localized_strings
                .get(&(
                    self.text_origin_file(id),
                    string_kind(self.tag_of(id), field),
                ))
                .and_then(|table| table.get(*string_id))
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("<string {string_id}>")),
        }
    }

    fn text_origin_file(&self, id: soe_esm::GlobalFormId) -> u16 {
        let origin = self
            .record_overrides
            .get(&id)
            .map_or(id, |record| record.text_origin);
        self.order
            .winner_ref(origin)
            .map_or(origin.file, |record| record.file as u16)
    }
}

fn string_kind(record: Option<Tag>, field: Tag) -> StringTableKind {
    if field == Tag(*b"DESC") || (record == Some(Tag(*b"QUST")) && field == Tag(*b"CNAM")) {
        StringTableKind::Dl
    } else if record == Some(Tag(*b"INFO")) && field != Tag(*b"RNAM") {
        StringTableKind::Il
    } else {
        StringTableKind::Plain
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use soe_esm::plugin::flags;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record};

    use super::*;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn table(kind: StringTableKind, id: u32, text: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        if kind.length_prefixed() {
            data.extend_from_slice(&((text.len() + 1) as u32).to_le_bytes());
        }
        data.extend_from_slice(text);
        data.push(0);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&id.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(data.as_slice());
        bytes
    }

    #[test]
    fn localized_dialogue_and_messages_use_their_winning_plugin_tables() {
        let root = std::env::temp_dir().join(format!(
            "soe-localization-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let strings = root.join("Strings");
        std::fs::create_dir_all(&strings).unwrap();
        std::fs::write(
            strings.join("Dialogue_english.strings"),
            table(StringTableKind::Plain, 1, b"What do you need?"),
        )
        .unwrap();
        std::fs::write(
            strings.join("Dialogue_english.ilstrings"),
            table(StringTableKind::Il, 2, b"Need something?"),
        )
        .unwrap();
        std::fs::write(
            strings.join("Dialogue_english.dlstrings"),
            table(StringTableKind::Dl, 3, b"The door is locked."),
        )
        .unwrap();
        std::fs::write(
            strings.join("Override_english.strings"),
            table(StringTableKind::Plain, 1, b"The override question"),
        )
        .unwrap();
        let mut plugin = Plugin::new(FileHeader {
            flags: flags::LOCALIZED,
            ..FileHeader::default()
        });
        plugin.push_top(Record::new(Tag(*b"DIAL"), 1).sub(Tag(*b"FULL"), 1_u32.to_le_bytes()));
        plugin.push_top(Record::new(Tag(*b"INFO"), 2).sub(Tag(*b"NAM1"), 2_u32.to_le_bytes()));
        plugin.push_top(Record::new(Tag(*b"MESG"), 3).sub(Tag(*b"DESC"), 3_u32.to_le_bytes()));
        let mut order = LoadOrder::new();
        order.add("Dialogue.esm", plugin).unwrap();
        let mut override_plugin = Plugin::new(FileHeader {
            flags: flags::LOCALIZED,
            masters: vec!["Dialogue.esm".into()],
            ..FileHeader::default()
        });
        override_plugin
            .push_top(Record::new(Tag(*b"DIAL"), 1).sub(Tag(*b"FULL"), 1_u32.to_le_bytes()));
        order.add("Override.esp", override_plugin).unwrap();
        let mut world = World::new(order);
        let mut vfs = Vfs::new();
        vfs.add_loose_root(&root).unwrap();
        assert_eq!(world.load_localized_strings(&vfs, "english").unwrap(), 4);
        assert_eq!(
            world.resolve_text(id(1), Tag(*b"FULL"), &Text::Localized(1)),
            "The override question"
        );
        assert_eq!(
            world.resolve_text(id(2), Tag(*b"NAM1"), &Text::Localized(2)),
            "Need something?"
        );
        assert_eq!(
            world.resolve_text(id(3), Tag(*b"DESC"), &Text::Localized(3)),
            "The door is locked."
        );
        std::fs::write(strings.join("Dialogue_french.strings"), [1_u8]).unwrap();
        vfs.add_loose_root(&root).unwrap();
        assert!(world.load_localized_strings(&vfs, "french").is_err());
        assert_eq!(
            world.resolve_text(id(1), Tag(*b"FULL"), &Text::Localized(1)),
            "The override question"
        );
        assert_eq!(world.load_localized_strings(&vfs, "german").unwrap(), 0);
        assert_eq!(
            world.resolve_text(id(1), Tag(*b"FULL"), &Text::Localized(1)),
            "<string 1>"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
