// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed view of a `PACK` record: an AI package. Layout from `wbRecord(PACK`
//! in `wbDefinitionsTES5.pas` and the UESP page
//! (`engine-refs/uesp-modfileformat/PACK.wikitext`), conditions from
//! `wbConditions` in the same file.
//!
//! A package is a template plus data. The template (`PKCU` names it, its
//! own `PKDT` type byte says it is one) carries the procedure tree, `ANAM`
//! branches with `PNAM` procedure names, and the data input names, `UNAM`
//! index with `BNAM` name. A package built on it carries values for those
//! inputs in its public data, `ANAM` type with the value subrecord, paired
//! with `UNAM` indexes in order. The same tag means different things in
//! different sections (`CNAM` is the combat style in the header and a
//! value in the public data, `PNAM` is a string in the tree and a flag in
//! the input names), so the reader is a state machine over the subrecord
//! list, the way `quest.rs` reads `QUST`.

use soe_esm::{Error, Record, Result, Tag};

pub const PACK: Tag = Tag::new(b"PACK");

/// `PKDT` general flags, the ones the Creation Kit shows.
pub mod flags {
    pub const MUST_COMPLETE: u32 = 0x0000_0004;
    pub const MAINTAIN_SPEED_AT_GOAL: u32 = 0x0000_0008;
    pub const UNLOCK_DOORS_AT_START: u32 = 0x0000_0040;
    pub const UNLOCK_DOORS_ON_CHANGE: u32 = 0x0000_0080;
    pub const ONCE_PER_DAY: u32 = 0x0000_0400;
    pub const PREFERRED_SPEED: u32 = 0x0000_2000;
    pub const ALWAYS_SNEAK: u32 = 0x0002_0000;
    pub const ALLOW_SWIMMING: u32 = 0x0004_0000;
    pub const IGNORE_COMBAT: u32 = 0x0010_0000;
    pub const WEAPONS_UNEQUIPPED: u32 = 0x0020_0000;
    pub const WEAPON_DRAWN: u32 = 0x0080_0000;
    pub const NO_COMBAT_ALERT: u32 = 0x0800_0000;
    pub const WEAR_SLEEP_OUTFIT: u32 = 0x2000_0000;
}

/// `PKDT` preferred speed.
pub mod speed {
    pub const WALK: u8 = 0;
    pub const JOG: u8 = 1;
    pub const RUN: u8 = 2;
    pub const FAST_WALK: u8 = 3;
}

/// The `CTDA` type byte: the comparison in the high three bits, flags in
/// the low five.
pub mod condition {
    pub const EQUAL: u8 = 0;
    pub const NOT_EQUAL: u8 = 1;
    pub const GREATER: u8 = 2;
    pub const GREATER_OR_EQUAL: u8 = 3;
    pub const LESS: u8 = 4;
    pub const LESS_OR_EQUAL: u8 = 5;

    pub const OR: u8 = 0x01;
    pub const USE_ALIASES: u8 = 0x02;
    pub const USE_GLOBAL: u8 = 0x04;
    pub const USE_PACK_DATA: u8 = 0x08;
    pub const SWAP_SUBJECT_AND_TARGET: u8 = 0x10;
}

/// `PKDT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct General {
    pub flags: u32,
    pub is_template: bool,
    pub interrupt_override: u8,
    pub preferred_speed: u8,
    pub interrupt_flags: u16,
}

/// `PSDT`. `-1` means any for month, day of week, hour and minute, `0`
/// means any for the date. Day of week 7 to 10 are the Creation Kit's
/// groups (weekdays, weekends, Morndas Middas Fredas, Tirdas Turdas).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub month: i8,
    pub day_of_week: i8,
    pub date: i8,
    pub hour: i8,
    pub minute: i8,
    pub duration_minutes: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            month: -1,
            day_of_week: -1,
            date: 0,
            hour: -1,
            minute: -1,
            duration_minutes: 0,
        }
    }
}

/// What a condition compares the function result against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CompValue {
    Float(f32),
    /// Raw FormID of a `GLOB`.
    Global(u32),
}

/// One `CTDA`. Raw FormIDs in the parameters resolve through the file the
/// record came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Condition {
    /// One of the `condition::EQUAL` to `condition::LESS_OR_EQUAL` codes.
    pub comparison: u8,
    /// The `condition::OR` and following bits.
    pub flags: u8,
    pub value: CompValue,
    /// Index into xEdit's `wbConditionFunctions` table.
    pub function: u16,
    pub param1: u32,
    pub param2: u32,
    /// 0 subject, 1 target, 2 reference, 3 combat target, 4 linked
    /// reference, 5 quest alias, 6 package data, 7 event data.
    pub run_on: u32,
    pub reference: u32,
    pub param3: i32,
}

impl Condition {
    pub fn is_or(&self) -> bool {
        self.flags & condition::OR != 0
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut d = Vec::with_capacity(32);
        d.push((self.comparison << 5) | (self.flags & 0x1F));
        d.extend_from_slice(&[0, 0, 0]);
        match self.value {
            CompValue::Float(v) => d.extend_from_slice(&v.to_le_bytes()),
            CompValue::Global(g) => d.extend_from_slice(&g.to_le_bytes()),
        }
        d.extend_from_slice(&self.function.to_le_bytes());
        d.extend_from_slice(&[0, 0]);
        d.extend_from_slice(&self.param1.to_le_bytes());
        d.extend_from_slice(&self.param2.to_le_bytes());
        d.extend_from_slice(&self.run_on.to_le_bytes());
        d.extend_from_slice(&self.reference.to_le_bytes());
        d.extend_from_slice(&self.param3.to_le_bytes());
        d
    }
}

/// `PLDT` location kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    NearReference(u32),
    InCell(u32),
    NearPackageStart,
    NearEditorLocation,
    /// The keyword the linked reference is chosen by, 0 for any.
    NearLinkedReference(u32),
    AliasReference(u32),
    AliasLocation(u32),
    NearSelf,
    Other(i32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocationData {
    pub location: Location,
    pub radius: i32,
}

/// `PTDA` target kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Reference(u32),
    ObjectId(u32),
    ObjectType(u32),
    LinkedReference(u32),
    Alias(u32),
    Itself,
    Other(i32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetData {
    pub target: Target,
    pub count: i32,
}

/// `PDTO` or `TPIC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Topic {
    Dialogue(u32),
    Subtype([u8; 4]),
}

/// The value after an `ANAM` in the public data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DataValue {
    None,
    Bool(bool),
    Int(i32),
    Float(f32),
    ObjectList(f32),
    Location(LocationData),
    Target(TargetData),
    Topic(Topic),
}

/// One public data entry: the `ANAM` type, its value, and the template
/// input index the trailing `UNAM` list assigned to it.
#[derive(Debug, Clone, PartialEq)]
pub struct DataInput {
    pub type_name: String,
    pub value: DataValue,
    pub index: Option<i8>,
}

/// One `ANAM` of the procedure tree. A branch with a `PNAM` is a procedure,
/// the others (`Random`, `Sequence`, `Simultaneous`, `Stacked`) group the
/// procedures that follow them, `child_count` many.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub kind: String,
    pub conditions: Vec<Condition>,
    pub child_count: u32,
    pub repeat_when_complete: bool,
    pub procedure: Option<String>,
    pub success_completes_package: bool,
    /// `PKC2` data input indexes the procedure reads, in its parameter order.
    pub inputs: Vec<u8>,
}

/// One data input declaration of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputName {
    pub index: i8,
    pub name: String,
    pub public: bool,
}

/// `POBA`, `POEA` or `POCA`: the idle and topic played at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Action {
    pub idle: Option<u32>,
    pub topic: Option<Topic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub general: General,
    pub schedule: Schedule,
    pub conditions: Vec<Condition>,
    /// `IDLA` idle animations, with the `IDLF` flags and `IDLT` timer.
    pub idles: Vec<u32>,
    pub idle_flags: u8,
    pub idle_timer: f32,
    pub combat_style: Option<u32>,
    pub owner_quest: Option<u32>,
    /// `PKCU`: the template this package fills in, 0 for none.
    pub template: Option<u32>,
    pub data_input_count: u32,
    pub inputs: Vec<DataInput>,
    pub tree: Vec<Branch>,
    pub input_names: Vec<InputName>,
    pub on_begin: Action,
    pub on_end: Action,
    pub on_change: Action,
}

enum Section {
    Header,
    PublicData,
    Tree,
    InputNames,
    OnBegin,
    OnEnd,
    OnChange,
}

impl Package {
    pub fn from_record(rec: &Record) -> Result<Package> {
        if rec.tag != PACK {
            return Err(Error::Invalid(format!("{} is not a PACK record", rec.tag)));
        }
        let mut p = Package {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            general: General::default(),
            schedule: Schedule::default(),
            conditions: Vec::new(),
            idles: Vec::new(),
            idle_flags: 0,
            idle_timer: 0.0,
            combat_style: None,
            owner_quest: None,
            template: None,
            data_input_count: 0,
            inputs: Vec::new(),
            tree: Vec::new(),
            input_names: Vec::new(),
            on_begin: Action::default(),
            on_end: Action::default(),
            on_change: Action::default(),
        };
        let mut section = Section::Header;
        let mut unam_seen = 0usize;
        for sub in &rec.subrecords {
            let d = sub.data.as_slice();
            match sub.tag.as_bytes() {
                b"POBA" => section = Section::OnBegin,
                b"POEA" => section = Section::OnEnd,
                b"POCA" => section = Section::OnChange,
                b"PKDT" => p.general = general(d)?,
                b"PSDT" => p.schedule = schedule(d)?,
                b"PKCU" => {
                    if d.len() < 12 {
                        return Err(short("PKCU"));
                    }
                    p.data_input_count = u32_at(d, 0);
                    let t = u32_at(d, 4);
                    p.template = (t != 0).then_some(t);
                    section = Section::PublicData;
                }
                b"XNAM" => section = Section::Tree,
                b"CTDA" => {
                    let c = parse_condition(d)?;
                    match section {
                        Section::Tree => {
                            if let Some(b) = p.tree.last_mut() {
                                b.conditions.push(c);
                            }
                        }
                        _ => p.conditions.push(c),
                    }
                }
                b"IDLA" => p
                    .idles
                    .extend(d.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c))),
                b"IDLF" => p.idle_flags = d.first().copied().unwrap_or(0),
                b"IDLT" if d.len() >= 4 => p.idle_timer = f32_at(d, 0),
                b"QNAM" => {
                    if matches!(section, Section::Header) {
                        p.owner_quest = Some(u32_at_checked(d, "QNAM")?);
                    }
                }
                b"INAM" => {
                    let idle = u32_at_checked(d, "INAM")?;
                    if let Some(a) = p.action_mut(&section) {
                        a.idle = (idle != 0).then_some(idle);
                    }
                }
                b"ANAM" => match section {
                    Section::PublicData => p.inputs.push(DataInput {
                        type_name: zstring(d),
                        value: DataValue::None,
                        index: None,
                    }),
                    Section::Tree => p.tree.push(Branch {
                        kind: zstring(d),
                        conditions: Vec::new(),
                        child_count: 0,
                        repeat_when_complete: false,
                        procedure: None,
                        success_completes_package: false,
                        inputs: Vec::new(),
                    }),
                    _ => {}
                },
                b"CNAM" => match section {
                    Section::Header => p.combat_style = Some(u32_at_checked(d, "CNAM")?),
                    Section::PublicData => {
                        if let Some(input) = p.inputs.last_mut() {
                            input.value = data_value(&input.type_name, d)?;
                        }
                    }
                    _ => {}
                },
                b"PLDT" => {
                    if let Some(input) = p.inputs.last_mut() {
                        input.value = DataValue::Location(location(d)?);
                    }
                }
                b"PTDA" => {
                    if let Some(input) = p.inputs.last_mut() {
                        input.value = DataValue::Target(target(d)?);
                    }
                }
                b"PDTO" => {
                    let t = topic(d)?;
                    match section {
                        Section::PublicData => {
                            if let Some(input) = p.inputs.last_mut() {
                                input.value = t.map_or(DataValue::None, DataValue::Topic);
                            }
                        }
                        _ => {
                            if let Some(a) = p.action_mut(&section) {
                                a.topic = t;
                            }
                        }
                    }
                }
                b"TPIC" => {
                    if let Some(input) = p.inputs.last_mut() {
                        input.value = DataValue::Topic(Topic::Dialogue(u32_at_checked(d, "TPIC")?));
                    }
                }
                b"UNAM" => {
                    let index = *d.first().ok_or_else(|| short("UNAM"))? as i8;
                    match section {
                        Section::PublicData => {
                            if let Some(input) = p.inputs.get_mut(unam_seen) {
                                input.index = Some(index);
                            }
                            unam_seen += 1;
                        }
                        Section::Tree | Section::InputNames => {
                            section = Section::InputNames;
                            p.input_names.push(InputName {
                                index,
                                name: String::new(),
                                public: false,
                            });
                        }
                        _ => {}
                    }
                }
                b"BNAM" => {
                    if matches!(section, Section::InputNames) {
                        if let Some(n) = p.input_names.last_mut() {
                            n.name = zstring(d);
                        }
                    }
                }
                b"PNAM" => match section {
                    Section::Tree => {
                        if let Some(b) = p.tree.last_mut() {
                            b.procedure = Some(zstring(d));
                        }
                    }
                    Section::InputNames => {
                        if let Some(n) = p.input_names.last_mut() {
                            n.public = u32_at_checked(d, "PNAM")? != 0;
                        }
                    }
                    _ => {}
                },
                b"PRCB" => {
                    if d.len() < 8 {
                        return Err(short("PRCB"));
                    }
                    if let Some(b) = p.tree.last_mut() {
                        b.child_count = u32_at(d, 0);
                        b.repeat_when_complete = u32_at(d, 4) & 1 != 0;
                    }
                }
                b"FNAM" => {
                    if matches!(section, Section::Tree) {
                        if let Some(b) = p.tree.last_mut() {
                            b.success_completes_package = u32_at_checked(d, "FNAM")? != 0;
                        }
                    }
                }
                b"PKC2" => {
                    if let Some(b) = p.tree.last_mut() {
                        b.inputs.push(*d.first().ok_or_else(|| short("PKC2"))?);
                    }
                }
                _ => {}
            }
        }
        branch_ends(&p.tree)?;
        Ok(p)
    }

    fn action_mut(&mut self, section: &Section) -> Option<&mut Action> {
        match section {
            Section::OnBegin => Some(&mut self.on_begin),
            Section::OnEnd => Some(&mut self.on_end),
            Section::OnChange => Some(&mut self.on_change),
            _ => None,
        }
    }

    pub fn is_template(&self) -> bool {
        self.general.is_template
    }

    pub fn has_flag(&self, flag: u32) -> bool {
        self.general.flags & flag != 0
    }

    /// The public data entry a template input index names.
    pub fn input(&self, index: i8) -> Option<&DataInput> {
        self.inputs.iter().find(|i| i.index == Some(index))
    }

    /// The input name a template declares for an index.
    pub fn input_name(&self, index: i8) -> Option<&str> {
        self.input_names
            .iter()
            .find(|n| n.index == index)
            .map(|n| n.name.as_str())
    }

    /// The procedures of the tree in order, group branches skipped.
    pub fn procedures(&self) -> impl Iterator<Item = &Branch> {
        self.tree.iter().filter(|b| b.procedure.is_some())
    }
}

/// Exclusive subtree ends in the preorder list, from xEdit's PRCB child counts.
pub(crate) fn branch_ends(tree: &[Branch]) -> Result<Vec<usize>> {
    let mut ends = vec![0; tree.len()];
    for (index, branch) in tree.iter().enumerate().rev() {
        if branch.procedure.is_some() && branch.child_count != 0 {
            return Err(Error::Invalid("PACK procedure has child branches".into()));
        }
        let mut end = index + 1;
        for _ in 0..branch.child_count {
            end = *ends
                .get(end)
                .ok_or_else(|| Error::Invalid("PACK branch child count exceeds its tree".into()))?;
        }
        ends[index] = end;
    }
    Ok(ends)
}

fn general(d: &[u8]) -> Result<General> {
    if d.len() < 12 {
        return Err(short("PKDT"));
    }
    Ok(General {
        flags: u32_at(d, 0),
        is_template: d[4] & 1 != 0,
        interrupt_override: d[5],
        preferred_speed: d[6],
        interrupt_flags: u16::from_le_bytes([d[8], d[9]]),
    })
}

fn schedule(d: &[u8]) -> Result<Schedule> {
    if d.len() < 12 {
        return Err(short("PSDT"));
    }
    Ok(Schedule {
        month: d[0] as i8,
        day_of_week: d[1] as i8,
        date: d[2] as i8,
        hour: d[3] as i8,
        minute: d[4] as i8,
        duration_minutes: u32_at(d, 8),
    })
}

fn parse_condition(d: &[u8]) -> Result<Condition> {
    if d.len() < 32 {
        return Err(short("CTDA"));
    }
    let flags = d[0] & 0x1F;
    let value = if flags & condition::USE_GLOBAL != 0 {
        CompValue::Global(u32_at(d, 4))
    } else {
        CompValue::Float(f32_at(d, 4))
    };
    Ok(Condition {
        comparison: d[0] >> 5,
        flags,
        value,
        function: u16::from_le_bytes([d[8], d[9]]),
        param1: u32_at(d, 12),
        param2: u32_at(d, 16),
        run_on: u32_at(d, 20),
        reference: u32_at(d, 24),
        param3: u32_at(d, 28) as i32,
    })
}

fn data_value(type_name: &str, d: &[u8]) -> Result<DataValue> {
    Ok(match type_name.to_ascii_lowercase().as_str() {
        "bool" => DataValue::Bool(*d.first().ok_or_else(|| short("CNAM"))? != 0),
        "int" => DataValue::Int(u32_at_checked(d, "CNAM")? as i32),
        "float" => DataValue::Float(f32::from_bits(u32_at_checked(d, "CNAM")?)),
        "objectlist" => DataValue::ObjectList(f32::from_bits(u32_at_checked(d, "CNAM")?)),
        _ => DataValue::None,
    })
}

fn location(d: &[u8]) -> Result<LocationData> {
    if d.len() < 12 {
        return Err(short("PLDT"));
    }
    let kind = u32_at(d, 0) as i32;
    let value = u32_at(d, 4);
    let location = match kind {
        0 => Location::NearReference(value),
        1 => Location::InCell(value),
        2 => Location::NearPackageStart,
        3 => Location::NearEditorLocation,
        6 => Location::NearLinkedReference(value),
        8 => Location::AliasReference(value),
        9 => Location::AliasLocation(value),
        12 => Location::NearSelf,
        other => Location::Other(other, value),
    };
    Ok(LocationData {
        location,
        radius: u32_at(d, 8) as i32,
    })
}

fn target(d: &[u8]) -> Result<TargetData> {
    if d.len() < 12 {
        return Err(short("PTDA"));
    }
    let kind = u32_at(d, 0) as i32;
    let value = u32_at(d, 4);
    let target = match kind {
        0 => Target::Reference(value),
        1 => Target::ObjectId(value),
        2 => Target::ObjectType(value),
        3 => Target::LinkedReference(value),
        4 => Target::Alias(value),
        6 => Target::Itself,
        other => Target::Other(other, value),
    };
    Ok(TargetData {
        target,
        count: u32_at(d, 8) as i32,
    })
}

fn topic(d: &[u8]) -> Result<Option<Topic>> {
    if d.len() < 8 {
        return Err(short("PDTO"));
    }
    let kind = u32_at(d, 0);
    let value = u32_at(d, 4);
    Ok(match kind {
        0 if value == 0 => None,
        0 => Some(Topic::Dialogue(value)),
        _ => Some(Topic::Subtype([d[4], d[5], d[6], d[7]])),
    })
}

fn u32_at(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

fn u32_at_checked(d: &[u8], tag: &str) -> Result<u32> {
    if d.len() < 4 {
        return Err(short(tag));
    }
    Ok(u32_at(d, 0))
}

fn f32_at(d: &[u8], i: usize) -> f32 {
    f32::from_bits(u32_at(d, i))
}

fn zstring(d: &[u8]) -> String {
    crate::quest::zstring(d)
}

fn short(tag: &str) -> Error {
    Error::Invalid(format!("PACK: {tag} is too short"))
}

// ----- builders for synthetic records -----

fn pkdt(flags: u32, is_template: bool, preferred_speed: u8) -> Vec<u8> {
    let mut d = flags.to_le_bytes().to_vec();
    d.push(if is_template { 0x13 } else { 0x12 });
    d.push(0);
    d.push(preferred_speed);
    d.push(0);
    d.extend_from_slice(&365u16.to_le_bytes());
    d.extend_from_slice(&[0, 0]);
    d
}

fn pkcu(template: Option<u32>) -> Vec<u8> {
    let mut d = 0u32.to_le_bytes().to_vec();
    d.extend_from_slice(&template.unwrap_or(0).to_le_bytes());
    d.extend_from_slice(&0u32.to_le_bytes());
    d
}

/// A package header with flags and a schedule. Conditions go in with
/// [`with_condition`], [`close_header`] names the template and opens the
/// public data ([`with_location_input`] and friends), [`finish`] appends
/// the `XNAM` marker and [`with_actions`] the three action blocks the
/// Creation Kit always writes.
pub fn build_record(form_id: u32, editor_id: &str, flags: u32, schedule: Schedule) -> Record {
    let mut psdt = vec![
        schedule.month as u8,
        schedule.day_of_week as u8,
        schedule.date as u8,
        schedule.hour as u8,
        schedule.minute as u8,
        0,
        0,
        0,
    ];
    psdt.extend_from_slice(&schedule.duration_minutes.to_le_bytes());
    Record::new(PACK, form_id)
        .zsub(Tag::new(b"EDID"), editor_id)
        .sub(Tag::new(b"PKDT"), pkdt(flags, false, speed::WALK))
        .sub(Tag::new(b"PSDT"), psdt)
}

/// A template record: `PKDT` flagged as a template, the procedure tree and
/// input names come from [`with_procedure`] and [`with_input_name`].
pub fn build_template(form_id: u32, editor_id: &str) -> Record {
    Record::new(PACK, form_id)
        .zsub(Tag::new(b"EDID"), editor_id)
        .sub(Tag::new(b"PKDT"), pkdt(0, true, speed::WALK))
        .sub(
            Tag::new(b"PSDT"),
            vec![0xFF, 0xFF, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0],
        )
}

/// A header condition. Must come before [`close_header`].
pub fn with_condition(rec: Record, c: &Condition) -> Record {
    rec.sub(Tag::new(b"CTDA"), c.to_bytes())
}

/// Ends the header (`PKCU`) so public data can follow.
pub fn close_header(rec: Record, template: Option<u32>) -> Record {
    rec.sub(Tag::new(b"PKCU"), pkcu(template))
}

pub fn with_location_input(rec: Record, index: i8, location: Location, radius: i32) -> Record {
    let (kind, value): (i32, u32) = match location {
        Location::NearReference(r) => (0, r),
        Location::InCell(c) => (1, c),
        Location::NearPackageStart => (2, 0),
        Location::NearEditorLocation => (3, 0),
        Location::NearLinkedReference(k) => (6, k),
        Location::AliasReference(a) => (8, a),
        Location::AliasLocation(a) => (9, a),
        Location::NearSelf => (12, 0),
        Location::Other(k, v) => (k, v),
    };
    let mut d = kind.to_le_bytes().to_vec();
    d.extend_from_slice(&value.to_le_bytes());
    d.extend_from_slice(&radius.to_le_bytes());
    rec.zsub(Tag::new(b"ANAM"), "Location")
        .sub(Tag::new(b"PLDT"), d)
        .sub(Tag::new(b"UNAM"), [index as u8])
}

pub fn with_bool_input(rec: Record, index: i8, value: bool) -> Record {
    rec.zsub(Tag::new(b"ANAM"), "Bool")
        .sub(Tag::new(b"CNAM"), [u8::from(value)])
        .sub(Tag::new(b"UNAM"), [index as u8])
}

pub fn with_float_input(rec: Record, index: i8, value: f32) -> Record {
    rec.zsub(Tag::new(b"ANAM"), "Float")
        .sub(Tag::new(b"CNAM"), value.to_le_bytes())
        .sub(Tag::new(b"UNAM"), [index as u8])
}

/// A procedure branch of a template tree. Call after [`finish`] on a
/// template, procedures live past the `XNAM` marker.
pub fn with_procedure(rec: Record, procedure: &str, inputs: &[u8], completes: bool) -> Record {
    let mut prcb = 0u32.to_le_bytes().to_vec();
    prcb.extend_from_slice(&0u32.to_le_bytes());
    let mut rec = rec
        .zsub(Tag::new(b"ANAM"), "Procedure")
        .sub(Tag::new(b"CITC"), 0u32.to_le_bytes())
        .sub(Tag::new(b"PRCB"), prcb)
        .zsub(Tag::new(b"PNAM"), procedure)
        .sub(Tag::new(b"FNAM"), u32::from(completes).to_le_bytes());
    for i in inputs {
        rec = rec.sub(Tag::new(b"PKC2"), [*i]);
    }
    rec
}

/// A data input declaration of a template, after its procedures.
pub fn with_input_name(rec: Record, index: i8, name: &str, public: bool) -> Record {
    rec.sub(Tag::new(b"UNAM"), [index as u8])
        .zsub(Tag::new(b"BNAM"), name)
        .sub(Tag::new(b"PNAM"), u32::from(public).to_le_bytes())
}

/// The `XNAM` marker that ends the public data.
pub fn finish(rec: Record) -> Record {
    rec.sub(Tag::new(b"XNAM"), [0u8])
}

/// The three action blocks every record ends with.
pub fn with_actions(rec: Record) -> Record {
    let empty = vec![0u8; 8];
    rec.sub(Tag::new(b"POBA"), Vec::new())
        .sub(Tag::new(b"INAM"), 0u32.to_le_bytes())
        .sub(Tag::new(b"PDTO"), empty.clone())
        .sub(Tag::new(b"POEA"), Vec::new())
        .sub(Tag::new(b"INAM"), 0u32.to_le_bytes())
        .sub(Tag::new(b"PDTO"), empty.clone())
        .sub(Tag::new(b"POCA"), Vec::new())
        .sub(Tag::new(b"INAM"), 0u32.to_le_bytes())
        .sub(Tag::new(b"PDTO"), empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn travel_template() -> Record {
        let rec = build_template(0x900, "TravelTemplate");
        let rec = close_header(rec, None);
        let rec = with_location_input(rec, 0, Location::NearSelf, 0);
        let rec = finish(rec);
        let rec = with_procedure(rec, "Travel", &[0], true);
        let rec = with_input_name(rec, 0, "Place to Travel", true);
        with_actions(rec)
    }

    fn travel_package() -> Record {
        let schedule = Schedule {
            hour: 8,
            duration_minutes: 240,
            ..Default::default()
        };
        let rec = build_record(0x901, "GuardTravel", flags::MUST_COMPLETE, schedule);
        let c = Condition {
            comparison: condition::GREATER_OR_EQUAL,
            flags: condition::OR,
            value: CompValue::Float(10.0),
            function: 58,
            param1: 0x800,
            param2: 0,
            run_on: 0,
            reference: 0,
            param3: -1,
        };
        let rec = with_condition(rec, &c).sub(Tag::new(b"CNAM"), 0x777u32.to_le_bytes());
        let rec = close_header(rec, Some(0x900));
        let rec = with_location_input(rec, 0, Location::NearReference(0x905), 256);
        let rec = with_bool_input(rec, 1, true);
        let rec = finish(rec);
        with_actions(rec).sub(Tag::new(b"INAM"), 0x333u32.to_le_bytes())
    }

    #[test]
    fn a_package_view_reads_header_data_and_actions() {
        let p = Package::from_record(&travel_package()).unwrap();
        assert_eq!(p.editor_id.as_deref(), Some("GuardTravel"));
        assert!(p.has_flag(flags::MUST_COMPLETE) && !p.is_template());
        assert_eq!(p.template, Some(0x900));
        assert_eq!(p.combat_style, Some(0x777));
        assert_eq!((p.schedule.hour, p.schedule.duration_minutes), (8, 240));
        assert_eq!(p.conditions.len(), 1);
        let c = p.conditions[0];
        assert_eq!(c.comparison, condition::GREATER_OR_EQUAL);
        assert!(c.is_or());
        assert_eq!(c.function, 58);
        assert_eq!(c.param1, 0x800);
        assert_eq!(c.value, CompValue::Float(10.0));
        assert_eq!(c.param3, -1);
        assert_eq!(p.inputs.len(), 2);
        assert_eq!(
            p.input(0).unwrap().value,
            DataValue::Location(LocationData {
                location: Location::NearReference(0x905),
                radius: 256
            })
        );
        assert_eq!(p.input(1).unwrap().value, DataValue::Bool(true));
        assert!(p.tree.is_empty());
        assert_eq!(
            p.on_change.idle,
            Some(0x333),
            "the last INAM lands on OnChange"
        );
        assert_eq!(p.on_begin.idle, None);
    }

    #[test]
    fn a_template_view_reads_the_tree_and_input_names() {
        let t = Package::from_record(&travel_template()).unwrap();
        assert!(t.is_template());
        assert_eq!(t.tree.len(), 1);
        let b = &t.tree[0];
        assert_eq!(b.procedure.as_deref(), Some("Travel"));
        assert_eq!(b.inputs, vec![0]);
        assert!(b.success_completes_package);
        assert_eq!(t.input_name(0), Some("Place to Travel"));
        assert!(t.input_names[0].public);
        assert_eq!(t.procedures().count(), 1);
        assert_eq!(
            t.input(0).unwrap().value,
            DataValue::Location(LocationData {
                location: Location::NearSelf,
                radius: 0
            })
        );
    }

    #[test]
    fn a_global_comparison_keeps_the_form_id() {
        let mut c = Condition {
            comparison: condition::EQUAL,
            flags: condition::USE_GLOBAL,
            value: CompValue::Global(0x123),
            function: 74,
            param1: 0x124,
            param2: 0,
            run_on: 2,
            reference: 0x14,
            param3: -1,
        };
        let back = parse_condition(&c.to_bytes()).unwrap();
        assert_eq!(back, c);
        c.flags = 0;
        c.value = CompValue::Float(2.5);
        assert_eq!(parse_condition(&c.to_bytes()).unwrap(), c);
    }

    #[test]
    fn malformed_blocks_are_refused() {
        let rec = Record::new(PACK, 1).sub(Tag::new(b"PKDT"), vec![0u8; 4]);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(PACK, 1).sub(Tag::new(b"PSDT"), vec![0u8; 4]);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(PACK, 1).sub(Tag::new(b"CTDA"), vec![0u8; 20]);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(PACK, 1).sub(Tag::new(b"PKCU"), vec![0u8; 8]);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(PACK, 1)
            .sub(Tag::new(b"PKCU"), pkcu(None))
            .zsub(Tag::new(b"ANAM"), "Location")
            .sub(Tag::new(b"PLDT"), vec![0u8; 6]);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(Tag::new(b"QUST"), 1);
        assert!(Package::from_record(&rec).is_err());
        let rec = Record::new(PACK, 1);
        let p = Package::from_record(&rec).unwrap();
        assert_eq!(p.schedule, Schedule::default());
    }
}
