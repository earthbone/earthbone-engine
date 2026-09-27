// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed view of a `QUST` record: flags, stages with their log entries,
//! objectives, and aliases. Layout from `wbRecord(QUST)` in
//! `wbDefinitionsTES5.pas`. The subrecords come in order and the meaning of
//! `FNAM` depends on whether an objective or an alias is open, so the reader
//! is a small state machine over the subrecord list. `ALFL` is the specific
//! location fill defined by xEdit and UESP's QUST format page.

use soe_esm::records::gameplay::Condition;
use soe_esm::records::Text;
use soe_esm::{Error, Record, Result, Tag};

use crate::vmad::Vmad;

pub mod state;

pub const QUST: Tag = Tag::new(b"QUST");

pub mod flags {
    pub const START_GAME_ENABLED: u16 = 0x0001;
    pub const ALLOW_REPEATED_STAGES: u16 = 0x0008;
    pub const RUN_ONCE: u16 = 0x0100;
}

pub mod stage_flags {
    pub const START_UP: u8 = 0x02;
    pub const SHUT_DOWN: u8 = 0x04;
}

pub mod entry_flags {
    pub const COMPLETE_QUEST: u8 = 0x01;
    pub const FAIL_QUEST: u8 = 0x02;
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub flags: u8,
    pub text: Option<Text>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub index: u16,
    pub flags: u8,
    pub entries: Vec<LogEntry>,
}

impl Stage {
    pub fn completes_quest(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.flags & entry_flags::COMPLETE_QUEST != 0)
    }
    pub fn fails_quest(&self) -> bool {
        self.entries
            .iter()
            .any(|e| e.flags & entry_flags::FAIL_QUEST != 0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    pub index: u16,
    pub flags: u32,
    pub text: Option<Text>,
    /// `(alias id, flags)` per QSTA target.
    pub targets: Vec<(i32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    pub id: u32,
    pub name: String,
    pub flags: u32,
    /// True for `ALLS` location aliases.
    pub location: bool,
    /// `ALFR`: the reference the alias holds from the start.
    pub forced_ref: Option<u32>,
    /// `ALFL`: the location the alias holds from the start.
    pub forced_location: Option<u32>,
    /// `ALPC`: packages the alias puts on its actor, in order.
    pub packages: Vec<u32>,
    /// `ALCO` selects Create Reference to Object rather than a placed reference.
    pub creates_reference: bool,
    /// `CTDA` predicates in the alias's Match Conditions block.
    pub match_conditions: Vec<Condition>,
}

pub mod alias_flags {
    //! Flags on reference and location aliases in a `QUST` record.
    pub const RESERVES_REFERENCE: u32 = 1 << 0;
    pub const OPTIONAL: u32 = 1 << 1;
    pub const ALLOW_REUSE_IN_QUEST: u32 = 1 << 3;
    pub const ALLOW_DEAD: u32 = 1 << 4;
    pub const ALLOW_DISABLED: u32 = 1 << 7;
    pub const ALLOW_RESERVED: u32 = 1 << 9;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quest {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub name: Option<Text>,
    pub flags: u16,
    pub priority: u8,
    pub quest_type: u32,
    pub stages: Vec<Stage>,
    pub objectives: Vec<Objective>,
    pub aliases: Vec<Alias>,
    pub vmad: Option<Vmad>,
    pub dialogue_conditions: Vec<Condition>,
}

enum Section {
    Header,
    Stage,
    Objective,
    Alias,
}

impl Quest {
    pub fn from_record(rec: &Record, localized: bool) -> Result<Quest> {
        if rec.tag != QUST {
            return Err(Error::Invalid(format!("{} is not a QUST record", rec.tag)));
        }
        let mut q = Quest {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            name: None,
            flags: 0,
            priority: 0,
            quest_type: 0,
            stages: Vec::new(),
            objectives: Vec::new(),
            aliases: Vec::new(),
            vmad: None,
            dialogue_conditions: Vec::new(),
        };
        let mut section = Section::Header;
        let mut dialogue_conditions = true;
        for sub in &rec.subrecords {
            if sub.tag == Tag(*b"NEXT") {
                dialogue_conditions = false;
            }
            if dialogue_conditions
                && matches!(section, Section::Header)
                && Condition::push(rec, sub, &mut q.dialogue_conditions)?
            {
                continue;
            }
            if matches!(section, Section::Alias) {
                if let Some(alias) = q.aliases.last_mut() {
                    if Condition::push(rec, sub, &mut alias.match_conditions)? {
                        continue;
                    }
                }
            }
            let d = sub.data.as_slice();
            match sub.tag.as_bytes() {
                b"VMAD" => q.vmad = Some(Vmad::parse(d, QUST)?),
                b"FULL" => q.name = Some(text(d, localized)),
                b"DNAM" => {
                    if d.len() < 4 {
                        return Err(short("DNAM"));
                    }
                    q.flags = u16::from_le_bytes([d[0], d[1]]);
                    q.priority = d[2];
                    if d.len() >= 12 {
                        q.quest_type = u32::from_le_bytes([d[8], d[9], d[10], d[11]]);
                    }
                }
                b"INDX" => {
                    if d.len() < 3 {
                        return Err(short("INDX"));
                    }
                    q.stages.push(Stage {
                        index: u16::from_le_bytes([d[0], d[1]]),
                        flags: d[2],
                        entries: Vec::new(),
                    });
                    section = Section::Stage;
                }
                b"QSDT" => {
                    let stage = q.stages.last_mut().ok_or_else(|| stray("QSDT"))?;
                    stage.entries.push(LogEntry {
                        flags: d.first().copied().unwrap_or(0),
                        text: None,
                    });
                }
                b"CNAM" => {
                    if let Some(entry) = q.stages.last_mut().and_then(|s| s.entries.last_mut()) {
                        entry.text = Some(text(d, localized));
                    }
                }
                b"QOBJ" => {
                    if d.len() < 2 {
                        return Err(short("QOBJ"));
                    }
                    q.objectives.push(Objective {
                        index: u16::from_le_bytes([d[0], d[1]]),
                        flags: 0,
                        text: None,
                        targets: Vec::new(),
                    });
                    section = Section::Objective;
                }
                b"NNAM" => {
                    if let Some(o) = q.objectives.last_mut() {
                        o.text = Some(text(d, localized));
                    }
                }
                b"QSTA" => {
                    if d.len() < 8 {
                        return Err(short("QSTA"));
                    }
                    if let Some(o) = q.objectives.last_mut() {
                        o.targets.push((
                            i32::from_le_bytes([d[0], d[1], d[2], d[3]]),
                            u32::from_le_bytes([d[4], d[5], d[6], d[7]]),
                        ));
                    }
                }
                b"ALST" | b"ALLS" => {
                    if d.len() < 4 {
                        return Err(short("ALST"));
                    }
                    q.aliases.push(Alias {
                        id: u32::from_le_bytes([d[0], d[1], d[2], d[3]]),
                        name: String::new(),
                        flags: 0,
                        location: sub.tag == Tag::new(b"ALLS"),
                        forced_ref: None,
                        forced_location: None,
                        packages: Vec::new(),
                        creates_reference: false,
                        match_conditions: Vec::new(),
                    });
                    section = Section::Alias;
                }
                b"ALPC" => {
                    if d.len() < 4 {
                        return Err(short("ALPC"));
                    }
                    if let Some(a) = q.aliases.last_mut() {
                        a.packages
                            .push(u32::from_le_bytes([d[0], d[1], d[2], d[3]]));
                    }
                }
                b"ALCO" => {
                    if let Some(a) = q.aliases.last_mut() {
                        a.creates_reference = true;
                    }
                }
                b"ALID" => {
                    if let Some(a) = q.aliases.last_mut() {
                        a.name = zstring(d);
                    }
                }
                b"ALFR" => {
                    if d.len() < 4 {
                        return Err(short("ALFR"));
                    }
                    if let Some(a) = q.aliases.last_mut() {
                        a.forced_ref = Some(u32::from_le_bytes([d[0], d[1], d[2], d[3]]));
                    }
                }
                b"ALFL" => {
                    if d.len() < 4 {
                        return Err(short("ALFL"));
                    }
                    if let Some(a) = q.aliases.last_mut() {
                        a.forced_location = Some(u32::from_le_bytes([d[0], d[1], d[2], d[3]]));
                    }
                }
                b"ALED" => section = Section::Header,
                b"FNAM" => {
                    if d.len() < 4 {
                        return Err(short("FNAM"));
                    }
                    let v = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
                    match section {
                        Section::Objective => {
                            if let Some(o) = q.objectives.last_mut() {
                                o.flags = v;
                            }
                        }
                        Section::Alias => {
                            if let Some(a) = q.aliases.last_mut() {
                                a.flags = v;
                            }
                        }
                        Section::Header | Section::Stage => {}
                    }
                }
                _ => {}
            }
        }
        Ok(q)
    }

    pub fn starts_enabled(&self) -> bool {
        self.flags & flags::START_GAME_ENABLED != 0
    }

    pub fn stage(&self, index: u16) -> Option<&Stage> {
        self.stages.iter().find(|s| s.index == index)
    }

    pub fn objective(&self, index: u16) -> Option<&Objective> {
        self.objectives.iter().find(|o| o.index == index)
    }

    pub fn alias(&self, id: u32) -> Option<&Alias> {
        self.aliases.iter().find(|a| a.id == id)
    }

    pub fn alias_named(&self, name: &str) -> Option<&Alias> {
        self.aliases
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// The stage flagged as the start-up stage, else the lowest one.
    pub fn startup_stage(&self) -> Option<u16> {
        self.stages
            .iter()
            .find(|s| s.flags & stage_flags::START_UP != 0)
            .or_else(|| self.stages.iter().min_by_key(|s| s.index))
            .map(|s| s.index)
    }
}

fn text(d: &[u8], localized: bool) -> Text {
    if localized && d.len() == 4 {
        Text::Localized(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
    } else {
        Text::Plain(zstring(d))
    }
}

pub(crate) fn zstring(d: &[u8]) -> String {
    let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
    d[..end].iter().map(|&b| b as char).collect()
}

fn short(tag: &str) -> Error {
    Error::Invalid(format!("QUST: {tag} is too short"))
}

fn stray(tag: &str) -> Error {
    Error::Invalid(format!("QUST: {tag} before any INDX"))
}

/// Builder for synthetic quest records, used by the tests here and in the
/// integration suite.
pub fn build_record(form_id: u32, editor_id: &str, quest_flags: u16) -> Record {
    let mut dnam = vec![0u8; 12];
    dnam[..2].copy_from_slice(&quest_flags.to_le_bytes());
    Record::new(QUST, form_id)
        .zsub(Tag::new(b"EDID"), editor_id)
        .sub(Tag::new(b"DNAM"), dnam)
}

pub fn with_stage(rec: Record, index: u16, stage_flags: u8, entry_flags: u8, log: &str) -> Record {
    let mut bytes = index.to_le_bytes().to_vec();
    bytes.push(stage_flags);
    bytes.push(0);
    rec.sub(Tag::new(b"INDX"), bytes)
        .sub(Tag::new(b"QSDT"), [entry_flags])
        .zsub(Tag::new(b"CNAM"), log)
}

pub fn with_objective(rec: Record, index: u16, text: &str) -> Record {
    rec.sub(Tag::new(b"QOBJ"), index.to_le_bytes())
        .sub(Tag::new(b"FNAM"), 0u32.to_le_bytes())
        .zsub(Tag::new(b"NNAM"), text)
}

pub fn with_alias(rec: Record, id: u32, name: &str, forced_ref: Option<u32>) -> Record {
    let rec = rec
        .sub(Tag::new(b"ALST"), id.to_le_bytes())
        .zsub(Tag::new(b"ALID"), name)
        .sub(Tag::new(b"FNAM"), 0u32.to_le_bytes());
    let rec = match forced_ref {
        Some(r) => rec.sub(Tag::new(b"ALFR"), r.to_le_bytes()),
        None => rec,
    };
    rec.sub(Tag::new(b"ALED"), Vec::new())
}

/// An alias with packages. Same shape as [`with_alias`], the `ALPC` list
/// sits before `ALED`.
pub fn with_alias_packages(
    rec: Record,
    id: u32,
    name: &str,
    forced_ref: Option<u32>,
    packages: &[u32],
) -> Record {
    let mut rec = rec
        .sub(Tag::new(b"ALST"), id.to_le_bytes())
        .zsub(Tag::new(b"ALID"), name)
        .sub(Tag::new(b"FNAM"), 0u32.to_le_bytes());
    if let Some(r) = forced_ref {
        rec = rec.sub(Tag::new(b"ALFR"), r.to_le_bytes());
    }
    for p in packages {
        rec = rec.sub(Tag::new(b"ALPC"), p.to_le_bytes());
    }
    rec.sub(Tag::new(b"ALED"), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmad::{ObjectRef, PropertyValue, ScriptEntry};

    #[test]
    fn a_quest_view_reads_stages_objectives_and_aliases() {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("MQ01Script").prop(
                "Lever",
                PropertyValue::Object(ObjectRef {
                    form_id: 0x803,
                    alias: -1,
                }),
            )],
            ..Default::default()
        };
        let rec = build_record(0x800, "MQ01", flags::START_GAME_ENABLED)
            .sub(Tag::new(b"VMAD"), vmad.write(QUST))
            .zsub(Tag::new(b"FULL"), "The Lever");
        let rec = with_stage(rec, 0, stage_flags::START_UP, 0, "");
        let rec = with_stage(rec, 10, 0, 0, "Pull the lever");
        let rec = with_stage(rec, 20, 0, entry_flags::COMPLETE_QUEST, "Done");
        let rec = with_objective(rec, 10, "Pull the lever");
        let rec = with_alias(rec, 0, "Lever", Some(0x803));
        let rec = with_alias(rec, 1, "Player", None);

        let q = Quest::from_record(&rec, false).unwrap();
        assert!(q.starts_enabled());
        assert_eq!(q.name, Some(Text::Plain("The Lever".into())));
        assert_eq!(q.stages.len(), 3);
        assert_eq!(q.startup_stage(), Some(0));
        assert!(q.stage(20).unwrap().completes_quest());
        assert!(!q.stage(10).unwrap().completes_quest());
        assert_eq!(
            q.stage(10).unwrap().entries[0].text,
            Some(Text::Plain("Pull the lever".into()))
        );
        assert_eq!(
            q.objective(10).unwrap().text,
            Some(Text::Plain("Pull the lever".into()))
        );
        assert_eq!(q.alias(0).unwrap().forced_ref, Some(0x803));
        assert_eq!(q.alias_named("player").unwrap().id, 1);
        assert_eq!(q.vmad.unwrap().scripts[0].name, "MQ01Script");
    }

    #[test]
    fn localized_text_keeps_the_string_id() {
        let rec = build_record(0x800, "MQ01", 0).sub(Tag::new(b"FULL"), 7u32.to_le_bytes());
        let q = Quest::from_record(&rec, true).unwrap();
        assert_eq!(q.name, Some(Text::Localized(7)));
    }

    #[test]
    fn malformed_subrecords_are_refused() {
        let rec = Record::new(QUST, 1).sub(Tag::new(b"INDX"), [0u8]);
        assert!(Quest::from_record(&rec, false).is_err());
        let rec = Record::new(QUST, 1).sub(Tag::new(b"QSDT"), [0u8]);
        assert!(Quest::from_record(&rec, false).is_err());
        let rec = Record::new(QUST, 1).sub(Tag::new(b"DNAM"), [0u8, 0]);
        assert!(Quest::from_record(&rec, false).is_err());
        let rec = Record::new(Tag::new(b"STAT"), 1);
        assert!(Quest::from_record(&rec, false).is_err());
    }
}
