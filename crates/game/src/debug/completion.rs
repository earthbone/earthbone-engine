// SPDX-License-Identifier: GPL-3.0-or-later

//! Completion uses the same command manifests and winning forms as execution.

use super::{catalog, Console, COMMANDS};
use crate::World;
use soe_esm::Tag;
use std::collections::BTreeSet;
use std::ops::Range;

/// Replacement token and deterministic matches for a console input caret.
pub struct Completion {
    /// Byte range of the token to replace, excluding a target prefix.
    pub range: Range<usize>,
    /// Command spellings, editor IDs or runtime FormIDs.
    pub candidates: Vec<String>,
}

#[derive(Clone, Copy)]
enum Kind {
    Reference,
    Owner,
    Base,
    Item,
    Form,
    Tagged(Tag),
}

impl Kind {
    fn accepts(self, world: &World, id: soe_esm::GlobalFormId) -> bool {
        let reference = world.reference_ref(id).is_some()
            || world
                .tag_of(id)
                .is_some_and(|tag| matches!(tag.as_bytes(), b"REFR" | b"ACHR"));
        match self {
            Self::Reference => reference,
            Self::Owner => world
                .tag_of(id)
                .is_some_and(|tag| matches!(tag.as_bytes(), b"NPC_" | b"FACT")),
            Self::Base => {
                !reference
                    && world.tag_of(id).is_some_and(|tag| {
                        matches!(
                            tag.as_bytes(),
                            b"STAT"
                                | b"TREE"
                                | b"ACTI"
                                | b"FURN"
                                | b"DOOR"
                                | b"CONT"
                                | b"FLOR"
                                | b"LIGH"
                                | b"MSTT"
                                | b"SCOL"
                                | b"TACT"
                                | b"MISC"
                                | b"WEAP"
                                | b"BOOK"
                                | b"INGR"
                                | b"ALCH"
                                | b"KEYM"
                                | b"SLGM"
                                | b"AMMO"
                                | b"SCRL"
                                | b"APPA"
                                | b"ARMO"
                                | b"NPC_"
                                | b"LVLN"
                                | b"LVLI"
                                | b"PROJ"
                                | b"HAZD"
                                | b"EXPL"
                                | b"IDLM"
                                | b"ADDN"
                                | b"ARTO"
                                | b"ASPC"
                                | b"SOUN"
                                | b"TXST"
                        )
                    })
            }
            Self::Item => world.tag_of(id).is_some_and(|tag| {
                matches!(
                    tag.as_bytes(),
                    b"MISC"
                        | b"WEAP"
                        | b"ARMO"
                        | b"BOOK"
                        | b"INGR"
                        | b"ALCH"
                        | b"KEYM"
                        | b"SLGM"
                        | b"AMMO"
                        | b"SCRL"
                        | b"APPA"
                )
            }),
            Self::Form => true,
            Self::Tagged(tag) => world.tag_of(id) == Some(tag),
        }
    }
}

fn argument_kind(command: &str, syntax: &str, index: usize) -> Option<Kind> {
    let mut rest = syntax;
    let mut arguments = Vec::new();
    while let Some(start) = rest.find(['<', '[']) {
        rest = &rest[start + 1..];
        let end = rest.find(['>', ']'])?;
        arguments.push(rest[..end].trim_matches(['<', '>']).trim());
        rest = &rest[end + 1..];
    }
    let word = arguments.get(index)?.to_ascii_lowercase();
    Some(match word.as_str() {
        "ref" | "reference" | "actor" | "target" | "actor ref" | "target ref"
        | "destination ref" | "killer" => Kind::Reference,
        "npc" | "npc base" | "actor base" => Kind::Tagged(Tag(*b"NPC_")),
        "npc base or faction" => Kind::Owner,
        "base" => Kind::Base,
        "item" => Kind::Item,
        "form" => Kind::Form,
        "name" | "name or id" if command == "lookup" => Kind::Form,
        "quest" => Kind::Tagged(Tag(*b"QUST")),
        "cell" => Kind::Tagged(Tag(*b"CELL")),
        "faction" => Kind::Tagged(Tag(*b"FACT")),
        "race" => Kind::Tagged(Tag(*b"RACE")),
        "spell" => Kind::Tagged(Tag(*b"SPEL")),
        "shout" => Kind::Tagged(Tag(*b"SHOU")),
        "perk" => Kind::Tagged(Tag(*b"PERK")),
        "word" => Kind::Tagged(Tag(*b"WOOP")),
        "keyword" => Kind::Tagged(Tag(*b"KYWD")),
        "global" => Kind::Tagged(Tag(*b"GLOB")),
        "location" => Kind::Tagged(Tag(*b"LCTN")),
        "weather" => Kind::Tagged(Tag(*b"WTHR")),
        "climate" => Kind::Tagged(Tag(*b"CLMT")),
        "worldspace" => Kind::Tagged(Tag(*b"WRLD")),
        "idle" => Kind::Tagged(Tag(*b"IDLE")),
        _ => return None,
    })
}

impl Console {
    /// Complete the token at a UTF-8 byte caret without executing a command.
    pub fn complete(world: &World, line: &str, caret: usize) -> Option<Completion> {
        let before = line.get(..caret)?;
        let separator = |character: char| character.is_whitespace() || character == ',';
        let mut start = before
            .char_indices()
            .rev()
            .find(|(_, character)| separator(*character))
            .map_or(0, |(at, character)| at + character.len_utf8());
        let mut end = line
            .get(caret..)?
            .find(separator)
            .map_or(line.len(), |at| caret + at);
        let words = super::play::tokenize(&before[..start]).ok()?;
        let mut candidates = BTreeSet::new();
        if words.is_empty() {
            if let Some(dot) = before[start..].rfind('.') {
                start += dot + 1;
            }
            let prefix = before[start..].to_ascii_lowercase();
            for entry in COMMANDS {
                if entry.name.starts_with(&prefix) {
                    candidates.insert(entry.name.to_owned());
                }
            }
            for entry in catalog::entries() {
                for name in std::iter::once(entry.name)
                    .chain(entry.aliases.split(',').filter(|alias| !alias.is_empty()))
                {
                    if name.starts_with(&prefix) {
                        candidates.insert(name.to_owned());
                    }
                }
            }
        } else {
            let lower = words[0].rsplit('.').next()?.to_ascii_lowercase();
            let command = catalog::canonical(&lower);
            let syntax = COMMANDS
                .iter()
                .find(|entry| entry.name == command)
                .map(|entry| entry.args)
                .or_else(|| catalog::entry(command).map(|entry| entry.syntax))?;
            let kind = argument_kind(command, syntax, words.len() - 1)?;
            if line.as_bytes().get(start) == Some(&b'"') {
                start += 1;
            }
            if end > start && line.as_bytes().get(end - 1) == Some(&b'"') {
                end -= 1;
            }
            let prefix = line
                .get(start..caret)?
                .trim_end_matches('"')
                .to_ascii_lowercase();
            let numeric_prefix = prefix.strip_prefix("0x").unwrap_or(&prefix);
            let short = u32::from_str_radix(numeric_prefix, 16).ok();
            if numeric_prefix.is_empty() || (numeric_prefix.len() <= 8 && short.is_some()) {
                for id in world.console_completion_forms() {
                    let raw = world.runtime_id(id);
                    let matches = numeric_prefix.is_empty()
                        || short == Some(raw)
                        || short
                            .is_some_and(|value| raw >> (4 * (8 - numeric_prefix.len())) == value);
                    if matches && kind.accepts(world, id) {
                        let numeric = format!("{raw:08X}");
                        candidates.insert(if prefix.starts_with("0x") {
                            format!("0x{numeric}")
                        } else {
                            numeric
                        });
                    }
                }
            }
            for id in world.console_editor_matches(&prefix) {
                if kind.accepts(world, id) {
                    if let Some(editor) = world.editor_id(id) {
                        candidates.insert(editor);
                    }
                }
            }
            if matches!(kind, Kind::Reference | Kind::Form) && "player".starts_with(&prefix) {
                candidates.insert("player".into());
            }
        }
        (!candidates.is_empty()).then(|| Completion {
            range: start..end,
            candidates: candidates.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};
    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        for (object, tag, editor) in [
            (0xf, *b"MISC", "Gold001"),
            (0x800, *b"REFR", "GiantReference"),
            (0x801, *b"NPC_", "GiantBase"),
        ] {
            plugin.push_top(Record::new(Tag(tag), object).zsub(b"EDID", editor));
        }
        let mut order = LoadOrder::new();
        order.add("Complete.esm", plugin).unwrap();
        World::new(order)
    }
    #[test]
    fn r4_completion_uses_command_names_aliases_and_target_prefixes() {
        let world = world();
        let line = "player.addi";
        let completion = Console::complete(&world, line, line.len()).unwrap();
        assert_eq!(completion.range, 7..11);
        assert!(completion.candidates.contains(&"additem".into()));
        assert!(Console::complete(&world, "tf", 2)
            .unwrap()
            .candidates
            .contains(&"tfc".into()));
    }
    #[test]
    fn r4_completion_filters_reference_and_base_arguments_and_keeps_the_suffix() {
        let world = world();
        let line = "prid Gia";
        assert_eq!(
            Console::complete(&world, line, line.len())
                .unwrap()
                .candidates,
            ["GiantReference"]
        );
        let line = "player.placeatme Gia";
        assert_eq!(
            Console::complete(&world, line, line.len())
                .unwrap()
                .candidates,
            ["GiantBase"]
        );
        let line = "player.additem gold0 7";
        let completion = Console::complete(&world, line, 20).unwrap();
        assert_eq!(completion.range, 15..20);
        assert_eq!(completion.candidates, ["Gold001"]);
        assert!(Console::complete(&world, "player.additem Gold001 7", 23).is_none());
    }
    #[test]
    fn r4_completion_resolves_runtime_formid_prefixes_and_short_hex() {
        let world = world();
        assert!(Console::complete(&world, "player.additem 0000000", 22)
            .unwrap()
            .candidates
            .contains(&"0000000F".into()));
        assert!(Console::complete(&world, "player.additem f", 16)
            .unwrap()
            .candidates
            .contains(&"0000000F".into()));
    }
    #[test]
    fn r4_completion_preserves_unicode_token_boundaries() {
        let world = world();
        let line = "player.additem\u{3000}gold0 7";
        let completion = Console::complete(&world, line, 22).unwrap();
        assert_eq!(completion.range, 17..22);
        assert_eq!(completion.candidates, ["Gold001"]);
        assert!(Console::complete(&world, line, 15).is_none());
        let comma_line = "player.additem,Gold0,7";
        let completion = Console::complete(&world, comma_line, 20).unwrap();
        assert_eq!(completion.range, 15..20);
        assert_eq!(completion.candidates, ["Gold001"]);
    }

    #[test]
    fn r4_completion_includes_runtime_created_reference_ids() {
        let mut world = world();
        let player = world.player();
        let reference = world.place_at(
            soe_esm::GlobalFormId {
                file: 0,
                object: 0xf,
            },
            player,
            false,
        );
        let expected = format!("{:08X}", world.runtime_id(reference));
        let completion = Console::complete(&world, "prid FF", 7).unwrap();
        assert!(completion.candidates.contains(&expected));
    }
    #[test]
    fn r4_completion_distinguishes_declared_ref_base_and_actor_value_arguments() {
        let world = world();
        for command in ["setessential", "setprotected", "setownership"] {
            let line = format!("{command} Gia");
            assert_eq!(
                Console::complete(&world, &line, line.len())
                    .unwrap()
                    .candidates,
                ["GiantBase"]
            );
        }
        for command in ["duplicateallitems", "removeallitems", "pushactoraway"] {
            let line = format!("{command} Gia");
            assert_eq!(
                Console::complete(&world, &line, line.len())
                    .unwrap()
                    .candidates,
                ["GiantReference"]
            );
        }
        let line = "damageav Gia";
        assert!(Console::complete(&world, line, line.len()).is_none());
    }
}
