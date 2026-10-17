// SPDX-License-Identifier: GPL-3.0-or-later

//! Quest text substitutions follow CK Text_Replacement and Show_-_Message.
//! xEdit QUST QTGL/FNAM and UESP MESG, RELA and ASTP supply the record links.
//! Substitution is one pass so inserted names cannot introduce new tags.

use soe_esm::{records::Text, GlobalFormId, Tag};
use soe_papyrus::Value;

use super::World;

enum AliasText {
    Filled(GlobalFormId),
    Empty,
}

impl AliasText {
    fn target(self) -> Option<GlobalFormId> {
        match self {
            Self::Filled(target) => Some(target),
            Self::Empty => None,
        }
    }
}

impl World {
    /// Render a Message's text and its nine numeric Show arguments.
    pub fn message_text(&self, message: GlobalFormId, arguments: &[f32]) -> String {
        let text = self
            .description_of(message)
            .or_else(|| self.name_of(message))
            .unwrap_or_default();
        format_numbers(&self.replace_text(message, None, &text), arguments)
    }
    /// Localize a record field, then expand its quest-associated display tokens.
    pub fn display_text(&self, owner: GlobalFormId, field: Tag, text: &Text) -> String {
        self.replace_text(owner, Some(owner), &self.resolve_text(owner, field, text))
    }

    /// Expand text in the owner's quest context and the named subject's context.
    /// Invalid tags remain literal. A declared but unfilled alias shows `[...]`.
    pub fn replace_text(
        &self,
        owner: GlobalFormId,
        subject: Option<GlobalFormId>,
        text: &str,
    ) -> String {
        if !text.contains('<') {
            return text.into();
        }
        let quest = self.text_quest(owner);
        let mut output = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('<') {
            output.push_str(&rest[..start]);
            let Some(end) = rest[start..].find('>') else {
                output.push_str(&rest[start..]);
                return output;
            };
            let token = &rest[start + 1..start + end];
            if let Some(value) = self.text_token(quest, subject, token) {
                output.push_str(&value);
            } else {
                output.push_str(&rest[start..=start + end]);
            }
            rest = &rest[start + end + 1..];
        }
        output.push_str(rest);
        output
    }

    fn text_quest(&self, owner: GlobalFormId) -> Option<GlobalFormId> {
        if self.tag_of(owner) == Some(Tag(*b"QUST")) {
            return Some(owner);
        }
        if let Some(quest) = self.owning_quest(owner) {
            return Some(quest);
        }
        self.loaded_quests().into_iter().find(|quest| {
            self.quest(*quest).is_some_and(|state| {
                state.def.aliases.iter().any(|alias| {
                    alias.flags & (1 << 14) != 0
                        && state
                            .aliases
                            .get(&alias.id)
                            .copied()
                            .flatten()
                            .is_some_and(|target| {
                                target == owner || self.text_base(target) == Some(owner)
                            })
                })
            })
        })
    }

    fn text_token(
        &self,
        quest: Option<GlobalFormId>,
        subject: Option<GlobalFormId>,
        token: &str,
    ) -> Option<String> {
        let lower = token.to_ascii_lowercase();
        if matches!(lower.as_str(), "basename" | "name") {
            let subject = subject?;
            return self.name_of(if lower == "basename" {
                self.text_base(subject).unwrap_or(subject)
            } else {
                subject
            });
        }
        let (key, name) = lower.split_once('=')?;
        let (kind, subtag) = key.split_once(['.', ' ']).unwrap_or((key, ""));
        let (subtag, cap) = subtag
            .strip_suffix("cap")
            .map_or((subtag, false), |s| (s, true));
        let value = match kind {
            "global" => self.text_global(quest?, name, subtag)?,
            "alias" => match self.text_alias(quest, name)? {
                AliasText::Filled(target) => self.text_alias_value(target, subtag)?,
                AliasText::Empty => "[...]".into(),
            },
            "relationship" => {
                let first = self.text_alias(quest, subtag)?.target()?;
                let second = self.text_alias(quest, name)?.target()?;
                self.text_relationship(first, second)?
            }
            _ => return None,
        };
        if cap {
            let mut chars = value.chars();
            Some(chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            }))
        } else {
            Some(value)
        }
    }

    fn text_alias(&self, quest: Option<GlobalFormId>, name: &str) -> Option<AliasText> {
        if name.eq_ignore_ascii_case("player") {
            return Some(AliasText::Filled(self.player.unwrap_or(GlobalFormId {
                file: 0,
                object: super::PLAYER_OBJECT,
            })));
        }
        let state = self.quest(quest?)?;
        let alias = state.def.alias_named(name)?;
        if alias.flags & (1 << 8) == 0 {
            return Some(AliasText::Empty);
        }
        let stored = state
            .journal
            .instances
            .get(&state.journal.current)
            .and_then(|instance| instance.aliases.get(&alias.id));
        Some(
            stored
                .or_else(|| state.aliases.get(&alias.id))
                .copied()
                .flatten()
                .map_or(AliasText::Empty, AliasText::Filled),
        )
    }

    fn text_base(&self, target: GlobalFormId) -> Option<GlobalFormId> {
        if let Some(reference) = self.refs.get(&target) {
            return self
                .actor_templates
                .get(&target)
                .and_then(|choice| choice.temporary_base)
                .or(reference.base);
        }
        let record = self.record(target)?;
        if matches!(record.tag.as_bytes(), b"ACHR" | b"REFR") {
            self.resolve_from(
                target,
                u32::from_le_bytes(record.find(Tag(*b"NAME"))?.try_into().ok()?),
            )
        } else {
            Some(target)
        }
    }

    fn text_traits(
        &self,
        target: GlobalFormId,
    ) -> Option<(GlobalFormId, bool, Option<GlobalFormId>)> {
        let base = self.text_base(target)?;
        let traits = self
            .npc_component_source(base, crate::npc::template::TRAITS)
            .ok()?;
        let npc = self.npc_record(traits).ok()?;
        let state = self.actor_bases.get(&traits);
        Some((
            base,
            state.map_or(npc.is_female(), |s| s.female),
            state
                .and_then(|s| s.race)
                .or_else(|| self.actor_base_race(base).ok().flatten()),
        ))
    }

    fn text_alias_value(&self, target: GlobalFormId, subtag: &str) -> Option<String> {
        if matches!(subtag, "" | "name" | "currentname") {
            return self.name_of(target);
        }
        if subtag == "basename" {
            return self.name_of(self.text_base(target)?);
        }
        if subtag == "shortname" {
            let base = self.text_base(target)?;
            return self
                .record(base)
                .and_then(|record| record.find(Tag(*b"SHRT")))
                .map(|raw| {
                    self.resolve_text(
                        base,
                        Tag(*b"SHRT"),
                        &super::text(raw, self.record_text_localized(base, Tag(*b"SHRT"))),
                    )
                })
                .filter(|name| !name.is_empty())
                .or_else(|| self.name_of(target));
        }
        let (_, female, race) = self.text_traits(target)?;
        if subtag == "race" {
            return self.name_of(race?);
        }
        let (setting, male, feminine) = match subtag {
            "gender" => ("sGender", "male", "female"),
            "pronoun" => ("sPronoun", "he", "she"),
            "pronounobj" => ("sPronounObj", "him", "her"),
            "pronounpos" => ("sPronounPos", "his", "hers"),
            "pronounposobj" => ("sPronounPosObj", "his", "her"),
            "pronounref" => ("sPronounRef", "himself", "herself"),
            "pronounint" => ("sPronounInt", "himself", "herself"),
            _ => return None,
        };
        Some(self.text_setting(
            &format!("{setting}{}", if female { "Female" } else { "Male" }),
            if female { feminine } else { male },
        ))
    }

    fn text_setting(&self, name: &str, fallback: &str) -> String {
        match self.game_setting(name) {
            Some(Value::Str(value)) => value.to_string(),
            _ => fallback.into(),
        }
    }

    fn text_global(&self, quest: GlobalFormId, name: &str, subtag: &str) -> Option<String> {
        let id = self.form(name)?;
        let record = self
            .record(id)
            .filter(|record| record.tag == Tag(*b"GLOB"))?;
        let value = *self.quest(quest)?.journal.globals()?.get(&id)?;
        if !value.is_finite() {
            return None;
        }
        if subtag.is_empty() {
            return Some(if record.find(Tag(*b"FNAM")) == Some(b"f") {
                format!("{value:.2}")
            } else {
                format!("{}", value as i32)
            });
        }
        if subtag == "time" {
            let hours = value >= 1.0;
            let count = if hours { value } else { value * 60.0 };
            let unit = self.text_setting(
                if hours { "sHours" } else { "sMinutes" },
                if hours { "Hours" } else { "Minutes" },
            );
            return Some(format!("{count:.0} {unit}"));
        }
        let clock = crate::clock::GameClock {
            game_days: f64::from(value),
            ..Default::default()
        };
        let calendar = crate::clock::Calendar::from_clock(&clock);
        let hour = calendar.hour.floor() as u32;
        Some(match subtag {
            "hour12" => (if hour.is_multiple_of(12) {
                12
            } else {
                hour % 12
            })
            .to_string(),
            "minutes" => format!("{:02}", (calendar.hour.fract() * 60.0).floor() as u32),
            "month" => (calendar.month + 1).to_string(),
            "day" => calendar.day.to_string(),
            "year" => calendar.year.to_string(),
            "monthword" => {
                const MONTHS: [&str; 12] = [
                    "Morning Star",
                    "Sun's Dawn",
                    "First Seed",
                    "Rain's Hand",
                    "Second Seed",
                    "Midyear",
                    "Sun's Height",
                    "Last Seed",
                    "Hearthfire",
                    "Frostfall",
                    "Sun's Dusk",
                    "Evening Star",
                ];
                const KEYS: [&str; 12] = [
                    "sMonthJanuary",
                    "sMonthFebruary",
                    "sMonthMarch",
                    "sMonthApril",
                    "sMonthMay",
                    "sMonthJune",
                    "sMonthJuly",
                    "sMonthAugust",
                    "sMonthSeptember",
                    "sMonthOctober",
                    "sMonthNovember",
                    "sMonthDecember",
                ];
                self.text_setting(
                    KEYS[calendar.month as usize],
                    MONTHS[calendar.month as usize],
                )
            }
            "weekday" => {
                const DAYS: [&str; 7] = [
                    "Sundas", "Morndas", "Tirdas", "Middas", "Turdas", "Fredas", "Loredas",
                ];
                const KEYS: [&str; 7] = [
                    "sDaySunday",
                    "sDayMonday",
                    "sDayTuesday",
                    "sDayWednesday",
                    "sDayThursday",
                    "sDayFriday",
                    "sDaySaturday",
                ];
                let day = (value.floor().max(0.0) as u64 % 7) as usize;
                self.text_setting(KEYS[day], DAYS[day])
            }
            "meridiem" => self.text_setting(
                if hour < 12 { "sAM" } else { "sPM" },
                if hour < 12 { "AM" } else { "PM" },
            ),
            "timespan" => {
                let span = match hour {
                    0..=5 => "Night",
                    6..=11 => "Morning",
                    12..=17 => "Afternoon",
                    _ => "Evening",
                };
                self.text_setting(&format!("sTime{span}"), span)
            }
            _ => return None,
        })
    }

    fn text_relationship(&self, first: GlobalFormId, second: GlobalFormId) -> Option<String> {
        let (first, female, _) = self.text_traits(first)?;
        let second = self.text_base(second)?;
        for (id, record) in self.order.winners_of(Tag(*b"RELA")) {
            let Some(data) = record.find(Tag(*b"DATA")).filter(|data| data.len() == 16) else {
                continue;
            };
            let form = |offset| {
                self.resolve_from(
                    id,
                    u32::from_le_bytes(data[offset..offset + 4].try_into().ok()?),
                )
            };
            let parent = form(0);
            let child = form(4);
            let parent_label = parent == Some(first) && child == Some(second);
            if !(parent_label || child == Some(first) && parent == Some(second)) {
                continue;
            }
            let association = form(12)?;
            let field = Tag(match (parent_label, female) {
                (true, false) => *b"MPRT",
                (true, true) => *b"FPRT",
                (false, false) => *b"MCHT",
                (false, true) => *b"FCHT",
            });
            let raw = self.record(association)?.find(field)?;
            return Some(self.resolve_text(
                association,
                field,
                &super::text(raw, self.record_text_localized(association, field)),
            ));
        }
        None
    }

    pub(super) fn snapshot_quest_text(&mut self, quest: GlobalFormId) {
        let globals: Vec<_> = self
            .record(quest)
            .into_iter()
            .flat_map(|record| record.subrecords.iter())
            .filter(|sub| sub.tag == Tag(*b"QTGL"))
            .filter_map(|sub| {
                self.resolve_from(
                    quest,
                    u32::from_le_bytes(sub.data.as_slice().try_into().ok()?),
                )
            })
            .collect();
        let globals = globals
            .into_iter()
            .map(|id| (id, self.global_value(id)))
            .collect();
        if let Some(state) = self.quests.get_mut(&quest) {
            if !state.journal.instances.is_empty() {
                state.journal.current = state.journal.current.saturating_add(1);
            }
            state.journal.instances.insert(
                state.journal.current,
                crate::quest::state::JournalInstance {
                    aliases: state
                        .def
                        .aliases
                        .iter()
                        .filter(|alias| alias.flags & (1 << 8) != 0)
                        .map(|alias| (alias.id, state.aliases.get(&alias.id).copied().flatten()))
                        .collect(),
                    globals,
                    ..Default::default()
                },
            );
        }
    }
}

fn format_numbers(text: &str, arguments: &[f32]) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    let mut argument = 0;
    while let Some(start) = rest.find('%') {
        output.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(tail) = rest.strip_prefix('%') {
            output.push('%');
            rest = tail;
            continue;
        }
        let flags_end = rest
            .find(|c: char| !matches!(c, '-' | '+' | ' ' | '#' | '0'))
            .unwrap_or(rest.len());
        let flags = &rest[..flags_end];
        let digits = &rest[flags_end..];
        let width_end = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        let width = digits[..width_end].parse::<usize>().unwrap_or(0).min(1024);
        let mut tail = &digits[width_end..];
        let mut precision = 6;
        let mut explicit_precision = false;
        if let Some(digits) = tail.strip_prefix('.') {
            explicit_precision = true;
            let end = digits
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(digits.len());
            precision = digits[..end].parse::<usize>().unwrap_or(0).min(32);
            tail = &digits[end..];
        }
        let Some(tail) = tail.strip_prefix('f') else {
            output.push('%');
            continue;
        };
        let value = arguments.get(argument).copied().unwrap_or(0.0);
        argument += 1;
        let mut number = if flags.contains('+') {
            format!("{value:+.precision$}")
        } else {
            format!("{value:.precision$}")
        };
        if !explicit_precision {
            while number.ends_with('0') && number.contains('.') {
                number.pop();
            }
            if number.ends_with('.') {
                number.push('0');
            }
        }
        if flags.contains('#') && !number.contains('.') {
            number.push('.');
        }
        if flags.contains(' ') && !flags.contains('+') && value >= 0.0 {
            number.insert(0, ' ');
        }
        let padding = width.saturating_sub(number.len());
        if flags.contains('-') {
            output.push_str(&number);
            output.extend(std::iter::repeat_n(' ', padding));
        } else if flags.contains('0') {
            if number.starts_with(['+', '-', ' ']) {
                output.push(number.remove(0));
            }
            output.extend(std::iter::repeat_n('0', padding));
            output.push_str(&number);
        } else {
            output.extend(std::iter::repeat_n(' ', padding));
            output.push_str(&number);
        }
        rest = tail;
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    const QUEST: GlobalFormId = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    const GLOBAL: GlobalFormId = GlobalFormId {
        file: 0,
        object: 0x801,
    };
    const ACTOR: GlobalFormId = GlobalFormId {
        file: 0,
        object: 0x802,
    };

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag(*b"GLOB"), GLOBAL.object)
                .zsub(Tag(*b"EDID"), "RoomCost")
                .sub(Tag(*b"FNAM"), b"s")
                .sub(Tag(*b"FLTV"), 10f32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"RACE"), 0x810).zsub(Tag(*b"FULL"), "Redguard"));
        let mut acbs = vec![0; 24];
        acbs[0] = 1;
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0x811)
                .zsub(Tag(*b"FULL"), "Saadia of Whiterun")
                .zsub(Tag(*b"SHRT"), "Saadia")
                .sub(Tag(*b"ACBS"), acbs)
                .sub(Tag(*b"RNAM"), 0x810u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), ACTOR.object)
                .sub(Tag(*b"NAME"), 0x811u32.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 0x812)
                .zsub(Tag(*b"FULL"), "Child")
                .sub(Tag(*b"ACBS"), vec![0; 24]),
        );
        plugin.push_top(
            Record::new(Tag(*b"ASTP"), 0x813)
                .zsub(Tag(*b"FPRT"), "Mother")
                .zsub(Tag(*b"MCHT"), "Son"),
        );
        let mut relation = Vec::new();
        for value in [0x811u32, 0x812, 0, 0x813] {
            relation.extend_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(Record::new(Tag(*b"RELA"), 0x814).sub(Tag(*b"DATA"), relation));
        plugin.push_top(
            Record::new(Tag(*b"GLOB"), 0x815)
                .zsub(Tag(*b"EDID"), "Amount")
                .sub(Tag(*b"FNAM"), b"f")
                .sub(Tag(*b"FLTV"), 1.25f32.to_le_bytes()),
        );
        plugin.push_top(Record::new(Tag(*b"BOOK"), 0x830).zsub(
            Tag(*b"DESC"),
            "Dear <Alias.ShortName=Innkeeper>, <Global=Amount>",
        ));
        plugin
            .push_top(Record::new(Tag(*b"REFR"), 0x831).sub(Tag(*b"NAME"), 0x830u32.to_le_bytes()));
        let quest = Record::new(Tag(*b"QUST"), QUEST.object)
            .sub(Tag(*b"DNAM"), vec![0; 12])
            .sub(Tag(*b"QTGL"), 0x815u32.to_le_bytes())
            .sub(Tag(*b"QTGL"), GLOBAL.object.to_le_bytes());
        let quest =
            crate::quest::with_alias_flags(quest, 0, "Innkeeper", Some(ACTOR.object), 1 << 8);
        let quest = crate::quest::with_alias_flags(quest, 1, "Empty", None, (1 << 8) | 2);
        let quest = crate::quest::with_alias_flags(quest, 2, "Letter", Some(0x831), 1 << 14);
        plugin.push_top(quest);
        plugin.push_top(
            Record::new(Tag(*b"MESG"), 0x820)
                .sub(Tag(*b"QNAM"), QUEST.object.to_le_bytes())
                .zsub(Tag(*b"DESC"), "Pay <Global=RoomCost> gold, %.0f%% now."),
        );
        let mut order = LoadOrder::new();
        order.add("Text.esm", plugin).unwrap();
        let mut world = World::new(order);
        world.quest_state(QUEST).unwrap();
        world.snapshot_quest_text(QUEST);
        world
    }

    #[test]
    fn globals_use_the_quest_snapshot_until_explicitly_updated() {
        let mut world = world();
        world.set_global(GLOBAL, 20.0);
        assert_eq!(world.replace_text(QUEST, None, "<Global=RoomCost>"), "10");
        world
            .quest_state(QUEST)
            .unwrap()
            .journal
            .update_global(GLOBAL, 20.0);
        assert_eq!(world.replace_text(QUEST, None, "<Global=RoomCost>"), "20");
    }

    #[test]
    fn aliases_render_names_race_all_pronouns_and_capitalization() {
        let world = world();
        for (tag, expected) in [
            ("", "Saadia of Whiterun"),
            (".ShortName", "Saadia"),
            (".Race", "Redguard"),
            (".Pronoun", "she"),
            (".PronounObj", "her"),
            (".PronounPos", "hers"),
            (".PronounPosObj", "her"),
            (".PronounRef", "herself"),
            (".PronounInt", "herself"),
            (".PronounCap", "She"),
        ] {
            assert_eq!(
                world.replace_text(QUEST, None, &format!("<Alias{tag}=Innkeeper>")),
                expected
            );
        }
    }

    #[test]
    fn unfilled_aliases_and_invalid_tokens_have_different_fallbacks() {
        let world = world();
        assert_eq!(world.replace_text(QUEST, None, "<Alias=Empty> <Alias=Missing> <Global=Absent> <Alias.Bad=Innkeeper> <b>word</b> <unfinished"),
            "[...] <Alias=Missing> <Global=Absent> <Alias.Bad=Innkeeper> <b>word</b> <unfinished");
    }

    #[test]
    fn alias_identity_comes_from_the_current_journal_instance() {
        let mut world = world();
        world.quest_state(QUEST).unwrap().aliases.insert(0, None);
        assert_eq!(
            world.replace_text(QUEST, None, "<Alias.ShortName=Innkeeper>"),
            "Saadia"
        );
        world.snapshot_quest_text(QUEST);
        assert_eq!(
            world.replace_text(QUEST, None, "<Alias.ShortName=Innkeeper>"),
            "[...]"
        );
    }

    #[test]
    fn base_name_ignores_reference_renaming_and_player_is_automatic() {
        let mut world = world();
        world.reference(ACTOR).unwrap().display_name = Some("Innkeeper".into());
        assert_eq!(
            world.replace_text(QUEST, Some(ACTOR), "<Name>: <BaseName>"),
            "Innkeeper: Saadia of Whiterun"
        );
        let player = world.player();
        world.reference(player).unwrap().display_name = Some("Dragonborn".into());
        assert_eq!(
            world.replace_text(QUEST, None, "<Alias=Player>"),
            "Dragonborn"
        );
    }

    #[test]
    fn date_and_timer_subtags_format_stored_game_days() {
        let mut world = world();
        world
            .quest_state(QUEST)
            .unwrap()
            .journal
            .update_global(GLOBAL, 0.25);
        for (tag, expected) in [
            ("Hour12", "6"),
            ("Minutes", "00"),
            ("Month", "8"),
            ("MonthWord", "Last Seed"),
            ("Day", "17"),
            ("WeekDay", "Sundas"),
            ("Year", "201"),
            ("TimeSpan", "Morning"),
            ("Meridiem", "AM"),
            ("Time", "15 Minutes"),
        ] {
            assert_eq!(
                world.replace_text(QUEST, None, &format!("<Global.{tag}=RoomCost>")),
                expected
            );
        }
    }

    #[test]
    fn message_numbers_follow_ck_width_precision_flags_and_literal_percent() {
        let world = world();
        assert_eq!(
            world.message_text(
                GlobalFormId {
                    file: 0,
                    object: 0x820
                },
                &[25.0]
            ),
            "Pay 10 gold, 25% now."
        );
        assert_eq!(
            format_numbers(
                "%+05.0f|%-6.2f|% .0f|%#.0f|%f|%q",
                &[5.0, 1.1, 2.0, 3.0, 5.0]
            ),
            "+0005|1.10  | 2|3.|5.0|%q"
        );
    }

    #[test]
    fn stored_text_survives_a_world_save_and_load() {
        let original = world();
        let saved = original.save_state();
        let mut restored = world();
        restored.load_state(&saved).unwrap();
        assert_eq!(
            restored.replace_text(QUEST, None, "<Alias.ShortName=Innkeeper> <Global=RoomCost>"),
            "Saadia 10"
        );
    }

    #[test]
    fn relationship_labels_follow_direction_and_sex() {
        let world = world();
        let child = GlobalFormId {
            file: 0,
            object: 0x812,
        };
        assert_eq!(
            world.text_relationship(ACTOR, child).as_deref(),
            Some("Mother")
        );
        assert_eq!(
            world.text_relationship(child, ACTOR).as_deref(),
            Some("Son")
        );
    }

    #[test]
    fn book_text_finds_its_stored_text_alias_and_formats_float_globals() {
        let world = world();
        let book = GlobalFormId {
            file: 0,
            object: 0x830,
        };
        assert_eq!(
            world.replace_text(book, Some(book), &world.description_of(book).unwrap()),
            "Dear Saadia, 1.25"
        );
    }
}
