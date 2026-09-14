// SPDX-License-Identifier: GPL-3.0-or-later

//! The `VMAD` subrecord: which scripts a record carries, the values the
//! Creation Kit filled into their properties, and for quests the stage
//! fragments and alias scripts.
//!
//! Layout per UESP's VMAD Field page as encoded in `wbDefinitionsTES5.pas`
//! (`wbVMAD`, `wbVMADFragmentedQUST`, `wbScriptPropertyObject`): a version
//! (2 to 5) and an object format (1 or 2), then a counted list of scripts. A
//! script is a length-prefixed name, a status byte from version 4 on, and a
//! counted list of properties. A property is a name, a type byte, a status
//! byte from version 4 on, and the value. Object values are eight bytes
//! whose order depends on the object format: v1 is FormID, alias, two unused
//! bytes, v2 is two unused bytes, alias, FormID. Arrays are a u32 count of
//! elements. Names are Windows-1252, read here as Latin-1.
//!
//! Only the quest tail (stage fragments and alias scripts) is decoded. The
//! fragment tails of INFO, PACK, PERK and SCEN records are kept as bytes in
//! `trailing` so the record still round-trips.

use soe_esm::{Error, Result, Tag};

pub const VERSION: i16 = 5;
pub const OBJECT_FORMAT: i16 = 2;

/// A form reference as VMAD writes it. `alias` is `-1` for a plain form and an
/// alias id when the property points at an alias on the quest `form_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectRef {
    pub form_id: u32,
    pub alias: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Object(ObjectRef),
    Str(String),
    Int(i32),
    Float(f32),
    Bool(bool),
    Objects(Vec<ObjectRef>),
    Strs(Vec<String>),
    Ints(Vec<i32>),
    Floats(Vec<f32>),
    Bools(Vec<bool>),
}

impl PropertyValue {
    pub fn type_byte(&self) -> u8 {
        match self {
            PropertyValue::Object(_) => 1,
            PropertyValue::Str(_) => 2,
            PropertyValue::Int(_) => 3,
            PropertyValue::Float(_) => 4,
            PropertyValue::Bool(_) => 5,
            PropertyValue::Objects(_) => 11,
            PropertyValue::Strs(_) => 12,
            PropertyValue::Ints(_) => 13,
            PropertyValue::Floats(_) => 14,
            PropertyValue::Bools(_) => 15,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    /// 1 edited, 3 removed.
    pub status: u8,
    pub value: PropertyValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptEntry {
    pub name: String,
    /// 0 local, 1 inherited, 3 removed.
    pub status: u8,
    pub properties: Vec<Property>,
}

impl ScriptEntry {
    pub fn new(name: &str) -> Self {
        ScriptEntry {
            name: name.to_string(),
            status: 0,
            properties: Vec::new(),
        }
    }

    pub fn prop(mut self, name: &str, value: PropertyValue) -> Self {
        self.properties.push(Property {
            name: name.to_string(),
            status: 1,
            value,
        });
        self
    }
}

/// A quest stage fragment: when `stage` is set, the game calls `function` on
/// the `script` attached to the quest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestFragment {
    pub stage: u16,
    pub log_entry: u32,
    pub script: String,
    pub function: String,
}

/// Scripts attached to one alias of a quest.
#[derive(Debug, Clone, PartialEq)]
pub struct AliasScripts {
    pub object: ObjectRef,
    pub scripts: Vec<ScriptEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vmad {
    pub version: i16,
    pub object_format: i16,
    pub scripts: Vec<ScriptEntry>,
    /// The fragment host script, `QF_<quest>_<id>`, when the quest has any.
    pub fragment_file: Option<String>,
    pub fragments: Vec<QuestFragment>,
    pub aliases: Vec<AliasScripts>,
    /// Undecoded fragment tail of non-quest records.
    pub trailing: Vec<u8>,
}

impl Default for Vmad {
    fn default() -> Self {
        Vmad {
            version: VERSION,
            object_format: OBJECT_FORMAT,
            scripts: Vec::new(),
            fragment_file: None,
            fragments: Vec::new(),
            aliases: Vec::new(),
            trailing: Vec::new(),
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    version: i16,
    object_format: i16,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| bad("size overflow"))?;
        if end > self.data.len() {
            return Err(bad(&format!(
                "needs {n} bytes at {} but only {} remain",
                self.pos,
                self.data.len() - self.pos
            )));
        }
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn wstring(&mut self) -> Result<String> {
        let n = self.u16()? as usize;
        Ok(latin1(self.take(n)?))
    }
    fn object(&mut self) -> Result<ObjectRef> {
        if self.object_format == 1 {
            let form_id = self.u32()?;
            let alias = self.i16()?;
            self.u16()?;
            Ok(ObjectRef { form_id, alias })
        } else {
            self.u16()?;
            let alias = self.i16()?;
            let form_id = self.u32()?;
            Ok(ObjectRef { form_id, alias })
        }
    }
    fn count(&mut self) -> Result<usize> {
        let n = self.u32()? as usize;
        // No element is smaller than a byte, so a count past the remaining
        // bytes is corrupt and must not be allocated.
        if n > self.data.len() - self.pos {
            return Err(bad(&format!("array count {n} exceeds the data")));
        }
        Ok(n)
    }
    fn value(&mut self, ty: u8) -> Result<PropertyValue> {
        Ok(match ty {
            1 => PropertyValue::Object(self.object()?),
            2 => PropertyValue::Str(self.wstring()?),
            3 => PropertyValue::Int(self.i32()?),
            4 => PropertyValue::Float(self.f32()?),
            5 => PropertyValue::Bool(self.u8()? != 0),
            11 => {
                let n = self.count()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.object()?);
                }
                PropertyValue::Objects(v)
            }
            12 => {
                let n = self.count()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.wstring()?);
                }
                PropertyValue::Strs(v)
            }
            13 => {
                let n = self.count()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.i32()?);
                }
                PropertyValue::Ints(v)
            }
            14 => {
                let n = self.count()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.f32()?);
                }
                PropertyValue::Floats(v)
            }
            15 => {
                let n = self.count()?;
                let mut v = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    v.push(self.u8()? != 0);
                }
                PropertyValue::Bools(v)
            }
            other => return Err(bad(&format!("unknown property type {other}"))),
        })
    }
    fn scripts(&mut self) -> Result<Vec<ScriptEntry>> {
        let n = self.u16()? as usize;
        let mut out = Vec::with_capacity(n.min(64));
        for _ in 0..n {
            let name = self.wstring()?;
            let status = if self.version >= 4 { self.u8()? } else { 0 };
            let pn = self.u16()? as usize;
            let mut properties = Vec::with_capacity(pn.min(64));
            for _ in 0..pn {
                let pname = self.wstring()?;
                let ty = self.u8()?;
                let pstatus = if self.version >= 4 { self.u8()? } else { 1 };
                let value = self.value(ty)?;
                properties.push(Property {
                    name: pname,
                    status: pstatus,
                    value,
                });
            }
            out.push(ScriptEntry {
                name,
                status,
                properties,
            });
        }
        Ok(out)
    }
}

fn bad(msg: &str) -> Error {
    Error::Invalid(format!("VMAD: {msg}"))
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

fn to_latin1(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
        .collect()
}

impl Vmad {
    /// Decode a `VMAD` subrecord. `tag` is the owning record's signature and
    /// picks the fragment layout.
    pub fn parse(data: &[u8], tag: Tag) -> Result<Vmad> {
        let mut r = Reader {
            data,
            pos: 0,
            version: 0,
            object_format: 0,
        };
        let version = r.i16()?;
        let object_format = r.i16()?;
        if !(2..=5).contains(&version) {
            return Err(bad(&format!("unsupported version {version}")));
        }
        if object_format != 1 && object_format != 2 {
            return Err(bad(&format!("unsupported object format {object_format}")));
        }
        r.version = version;
        r.object_format = object_format;
        let scripts = r.scripts()?;
        let mut vmad = Vmad {
            version,
            object_format,
            scripts,
            ..Default::default()
        };
        if r.pos == data.len() {
            return Ok(vmad);
        }
        if tag != Tag::new(b"QUST") {
            vmad.trailing = data[r.pos..].to_vec();
            return Ok(vmad);
        }
        let _extra_version = r.u8()?;
        let fragment_count = r.u16()? as usize;
        vmad.fragment_file = Some(r.wstring()?);
        for _ in 0..fragment_count {
            let stage = r.u16()?;
            r.u16()?;
            let log_entry = r.u32()?;
            r.u8()?;
            let script = r.wstring()?;
            let function = r.wstring()?;
            vmad.fragments.push(QuestFragment {
                stage,
                log_entry,
                script,
                function,
            });
        }
        let alias_count = r.u16()? as usize;
        for _ in 0..alias_count {
            let object = r.object()?;
            let v = r.i16()?;
            let f = r.i16()?;
            if !(2..=5).contains(&v) || (f != 1 && f != 2) {
                return Err(bad(&format!("alias block has version {v} format {f}")));
            }
            let saved = (r.version, r.object_format);
            r.version = v;
            r.object_format = f;
            let scripts = r.scripts()?;
            (r.version, r.object_format) = saved;
            vmad.aliases.push(AliasScripts { object, scripts });
        }
        if r.pos != data.len() {
            return Err(bad(&format!("{} trailing bytes", data.len() - r.pos)));
        }
        Ok(vmad)
    }

    /// Encode with version 5 and object format 2, the layout the Creation
    /// Kit writes for Skyrim SE.
    pub fn write(&self, tag: Tag) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&OBJECT_FORMAT.to_le_bytes());
        write_scripts(&mut out, &self.scripts);
        let has_tail = self.fragment_file.is_some() || !self.aliases.is_empty();
        if tag == Tag::new(b"QUST") && has_tail {
            out.push(2);
            out.extend_from_slice(&(self.fragments.len() as u16).to_le_bytes());
            write_wstring(&mut out, self.fragment_file.as_deref().unwrap_or(""));
            for f in &self.fragments {
                out.extend_from_slice(&f.stage.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&f.log_entry.to_le_bytes());
                out.push(1);
                write_wstring(&mut out, &f.script);
                write_wstring(&mut out, &f.function);
            }
            out.extend_from_slice(&(self.aliases.len() as u16).to_le_bytes());
            for a in &self.aliases {
                write_object(&mut out, a.object);
                out.extend_from_slice(&VERSION.to_le_bytes());
                out.extend_from_slice(&OBJECT_FORMAT.to_le_bytes());
                write_scripts(&mut out, &a.scripts);
            }
        } else {
            out.extend_from_slice(&self.trailing);
        }
        out
    }

    pub fn script(&self, name: &str) -> Option<&ScriptEntry> {
        self.scripts
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }
}

fn write_wstring(out: &mut Vec<u8>, s: &str) {
    let b = to_latin1(s);
    out.extend_from_slice(&(b.len() as u16).to_le_bytes());
    out.extend_from_slice(&b);
}

fn write_object(out: &mut Vec<u8>, o: ObjectRef) {
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&o.alias.to_le_bytes());
    out.extend_from_slice(&o.form_id.to_le_bytes());
}

fn write_scripts(out: &mut Vec<u8>, scripts: &[ScriptEntry]) {
    out.extend_from_slice(&(scripts.len() as u16).to_le_bytes());
    for s in scripts {
        write_wstring(out, &s.name);
        out.push(s.status);
        out.extend_from_slice(&(s.properties.len() as u16).to_le_bytes());
        for p in &s.properties {
            write_wstring(out, &p.name);
            out.push(p.value.type_byte());
            out.push(p.status);
            match &p.value {
                PropertyValue::Object(o) => write_object(out, *o),
                PropertyValue::Str(s) => write_wstring(out, s),
                PropertyValue::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
                PropertyValue::Float(f) => out.extend_from_slice(&f.to_le_bytes()),
                PropertyValue::Bool(b) => out.push(u8::from(*b)),
                PropertyValue::Objects(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for o in v {
                        write_object(out, *o);
                    }
                }
                PropertyValue::Strs(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for s in v {
                        write_wstring(out, s);
                    }
                }
                PropertyValue::Ints(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for i in v {
                        out.extend_from_slice(&i.to_le_bytes());
                    }
                }
                PropertyValue::Floats(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for f in v {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                }
                PropertyValue::Bools(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for b in v {
                        out.push(u8::from(*b));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUST: Tag = Tag::new(b"QUST");
    const REFR: Tag = Tag::new(b"REFR");

    fn sample() -> Vmad {
        Vmad {
            scripts: vec![
                ScriptEntry::new("MQ01Script")
                    .prop(
                        "Lever",
                        PropertyValue::Object(ObjectRef {
                            form_id: 0x803,
                            alias: -1,
                        }),
                    )
                    .prop("Greeting", PropertyValue::Str("Hallo".into()))
                    .prop("Count", PropertyValue::Int(-3))
                    .prop("Ratio", PropertyValue::Float(0.5))
                    .prop("Armed", PropertyValue::Bool(true))
                    .prop("Ids", PropertyValue::Ints(vec![1, 2, 3]))
                    .prop("Names", PropertyValue::Strs(vec!["a".into(), "bé".into()]))
                    .prop(
                        "Refs",
                        PropertyValue::Objects(vec![ObjectRef {
                            form_id: 0x800,
                            alias: 2,
                        }]),
                    )
                    .prop("Weights", PropertyValue::Floats(vec![1.5]))
                    .prop("Flags", PropertyValue::Bools(vec![true, false])),
                ScriptEntry::new("QF_MQ01_00000800"),
            ],
            fragment_file: Some("QF_MQ01_00000800".into()),
            fragments: vec![QuestFragment {
                stage: 10,
                log_entry: 0,
                script: "QF_MQ01_00000800".into(),
                function: "Fragment_0".into(),
            }],
            aliases: vec![AliasScripts {
                object: ObjectRef {
                    form_id: 0x800,
                    alias: 0,
                },
                scripts: vec![ScriptEntry::new("LeverAliasScript")],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_quest_vmad_round_trips() {
        let v = sample();
        let bytes = v.write(QUST);
        let back = Vmad::parse(&bytes, QUST).unwrap();
        assert_eq!(back, v);
        assert_eq!(back.script("mq01script").unwrap().properties.len(), 10);
    }

    #[test]
    fn a_plain_vmad_keeps_an_unknown_tail() {
        let v = Vmad {
            scripts: vec![ScriptEntry::new("A")],
            trailing: vec![9, 9, 9],
            ..Default::default()
        };
        let bytes = v.write(REFR);
        let back = Vmad::parse(&bytes, REFR).unwrap();
        assert_eq!(back.trailing, vec![9, 9, 9]);
        assert_eq!(back.write(REFR), bytes);
    }

    #[test]
    fn object_format_one_swaps_the_field_order() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&5i16.to_le_bytes());
        bytes.extend_from_slice(&1i16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut bytes, "S");
        bytes.push(0);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut bytes, "P");
        bytes.push(1);
        bytes.push(1);
        bytes.extend_from_slice(&0x1234u32.to_le_bytes());
        bytes.extend_from_slice(&(-1i16).to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        let v = Vmad::parse(&bytes, REFR).unwrap();
        assert_eq!(
            v.scripts[0].properties[0].value,
            PropertyValue::Object(ObjectRef {
                form_id: 0x1234,
                alias: -1
            })
        );
    }

    #[test]
    fn version_three_has_no_status_bytes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&3i16.to_le_bytes());
        bytes.extend_from_slice(&2i16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut bytes, "S");
        bytes.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut bytes, "N");
        bytes.push(3);
        bytes.extend_from_slice(&7i32.to_le_bytes());
        let v = Vmad::parse(&bytes, REFR).unwrap();
        assert_eq!(v.scripts[0].properties[0].value, PropertyValue::Int(7));
    }

    #[test]
    fn malformed_input_is_refused_without_allocating() {
        let good = sample().write(QUST);
        for cut in [0, 3, 4, 7, 12, 40, good.len() - 1] {
            assert!(Vmad::parse(&good[..cut], QUST).is_err(), "cut at {cut}");
        }
        let mut huge = Vec::new();
        huge.extend_from_slice(&5i16.to_le_bytes());
        huge.extend_from_slice(&2i16.to_le_bytes());
        huge.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut huge, "S");
        huge.push(0);
        huge.extend_from_slice(&1u16.to_le_bytes());
        write_wstring(&mut huge, "P");
        huge.push(13);
        huge.push(1);
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        let err = Vmad::parse(&huge, REFR).unwrap_err().to_string();
        assert!(err.contains("array count"), "{err}");
        let mut bad_type = huge.clone();
        let at = huge.len() - 5;
        bad_type[at] = 9;
        assert!(Vmad::parse(&bad_type, REFR).is_err());
        let mut bad_version = good.clone();
        bad_version[0] = 9;
        assert!(Vmad::parse(&bad_version, QUST).is_err());
    }
}
