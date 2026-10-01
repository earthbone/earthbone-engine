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
//! Quest, INFO, PACK and SCEN tails are decoded. PACK follows xEdit's
//! `wbVMADFragmentedPACK` (`wbScriptFragmentsPackCounter` for the fragment
//! count) and Mutagen's `PackageScriptFragments`: an extra bind data
//! version, a flags byte for `OnBegin`, `OnEnd` and `OnChange`, the fragment
//! host file name, then a begin, end and change fragment in that fixed order
//! for each flag bit set. SCEN follows xEdit's `wbVMADFragmentedSCEN` and
//! FallrimTools' `FragmentScen`: begin and end entries precede a counted
//! list of phase entries. PERK tails remain in `trailing`.

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

/// A begin or end callback, including its retained VMAD marker byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoFragment {
    pub marker: u8,
    pub script: String,
    pub function: String,
}

/// INFO fragment layout from xEdit wbVMADFragmentedINFO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoFragments {
    pub version: u8,
    pub file: String,
    pub begin: Option<InfoFragment>,
    pub end: Option<InfoFragment>,
}

/// One PACK fragment, retaining its per-fragment extra bind data version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFragment {
    pub marker: u8,
    pub script: String,
    pub function: String,
}

/// PACK fragment layout from xEdit's `wbVMADFragmentedPACK`: begin, end and
/// change entries follow the flags that mark them present, always in that
/// order, and a missing entry contributes no bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFragments {
    pub version: u8,
    pub file: String,
    pub begin: Option<PackFragment>,
    pub end: Option<PackFragment>,
    pub change: Option<PackFragment>,
}

/// A SCEN phase callback with its authored flag, phase index and marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenePhaseFragment {
    pub flag: u8,
    pub phase_index: u32,
    pub marker: u8,
    pub script: String,
    pub function: String,
}

/// SCEN begin, end and ordered phase callbacks from its VMAD tail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneFragments {
    pub version: u8,
    pub file: String,
    pub begin: Option<InfoFragment>,
    pub end: Option<InfoFragment>,
    pub phases: Vec<ScenePhaseFragment>,
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
    pub info_fragments: Option<InfoFragments>,
    pub pack_fragments: Option<PackFragments>,
    pub scene_fragments: Option<SceneFragments>,
    /// Undecoded fragment tail of PERK and other unsupported records.
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
            info_fragments: None,
            pack_fragments: None,
            scene_fragments: None,
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
        if tag == Tag::new(b"INFO") {
            let version = r.u8()?;
            let flags = r.u8()?;
            if version != 2 || flags & !3 != 0 {
                return Err(bad("unsupported INFO fragment version or flags"));
            }
            let file = r.wstring()?;
            let mut fragment = || -> Result<InfoFragment> {
                Ok(InfoFragment {
                    marker: r.u8()?,
                    script: r.wstring()?,
                    function: r.wstring()?,
                })
            };
            let begin = if flags & 1 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            let end = if flags & 2 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            if r.pos != data.len() {
                return Err(bad("trailing INFO fragment bytes"));
            }
            vmad.info_fragments = Some(InfoFragments {
                version,
                file,
                begin,
                end,
            });
            return Ok(vmad);
        }
        if tag == Tag::new(b"PACK") {
            let version = r.u8()?;
            let flags = r.u8()?;
            if flags & !7 != 0 {
                return Err(bad("unsupported PACK fragment flags"));
            }
            let file = r.wstring()?;
            let mut fragment = || -> Result<PackFragment> {
                Ok(PackFragment {
                    marker: r.u8()?,
                    script: r.wstring()?,
                    function: r.wstring()?,
                })
            };
            let begin = if flags & 1 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            let end = if flags & 2 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            let change = if flags & 4 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            if r.pos != data.len() {
                return Err(bad("trailing PACK fragment bytes"));
            }
            vmad.pack_fragments = Some(PackFragments {
                version,
                file,
                begin,
                end,
                change,
            });
            return Ok(vmad);
        }
        if tag == Tag::new(b"SCEN") {
            let version = r.u8()?;
            let flags = r.u8()?;
            if flags & !3 != 0 {
                return Err(bad("unsupported SCEN fragment flags"));
            }
            let file = r.wstring()?;
            let mut fragment = || -> Result<InfoFragment> {
                Ok(InfoFragment {
                    marker: r.u8()?,
                    script: r.wstring()?,
                    function: r.wstring()?,
                })
            };
            let begin = if flags & 1 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            let end = if flags & 2 != 0 {
                Some(fragment()?)
            } else {
                None
            };
            let count = r.u16()? as usize;
            let mut phases = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                phases.push(ScenePhaseFragment {
                    flag: r.u8()?,
                    phase_index: r.u32()?,
                    marker: r.u8()?,
                    script: r.wstring()?,
                    function: r.wstring()?,
                });
            }
            if r.pos != data.len() {
                return Err(bad("trailing SCEN fragment bytes"));
            }
            vmad.scene_fragments = Some(SceneFragments {
                version,
                file,
                begin,
                end,
                phases,
            });
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

    /// Encode with the version and object format `parse` read (a freshly
    /// built `Vmad` defaults to version 5, object format 2, the layout the
    /// Creation Kit writes for Skyrim SE), so a parsed record round-trips
    /// even when it predates that layout.
    pub fn write(&self, tag: Tag) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.object_format.to_le_bytes());
        write_scripts(&mut out, &self.scripts, self.object_format);
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
                write_object(&mut out, a.object, self.object_format);
                out.extend_from_slice(&VERSION.to_le_bytes());
                out.extend_from_slice(&OBJECT_FORMAT.to_le_bytes());
                write_scripts(&mut out, &a.scripts, self.object_format);
            }
        } else if let Some(info) = self
            .info_fragments
            .as_ref()
            .filter(|_| tag == Tag::new(b"INFO"))
        {
            out.push(info.version);
            out.push(u8::from(info.begin.is_some()) | (u8::from(info.end.is_some()) << 1));
            write_wstring(&mut out, &info.file);
            for fragment in [&info.begin, &info.end].into_iter().flatten() {
                out.push(fragment.marker);
                write_wstring(&mut out, &fragment.script);
                write_wstring(&mut out, &fragment.function);
            }
        } else if let Some(pack) = self
            .pack_fragments
            .as_ref()
            .filter(|_| tag == Tag::new(b"PACK"))
        {
            out.push(pack.version);
            out.push(
                u8::from(pack.begin.is_some())
                    | (u8::from(pack.end.is_some()) << 1)
                    | (u8::from(pack.change.is_some()) << 2),
            );
            write_wstring(&mut out, &pack.file);
            for fragment in [&pack.begin, &pack.end, &pack.change].into_iter().flatten() {
                out.push(fragment.marker);
                write_wstring(&mut out, &fragment.script);
                write_wstring(&mut out, &fragment.function);
            }
        } else if let Some(scene) = self
            .scene_fragments
            .as_ref()
            .filter(|_| tag == Tag::new(b"SCEN"))
        {
            out.push(scene.version);
            out.push(u8::from(scene.begin.is_some()) | (u8::from(scene.end.is_some()) << 1));
            write_wstring(&mut out, &scene.file);
            for fragment in [&scene.begin, &scene.end].into_iter().flatten() {
                out.push(fragment.marker);
                write_wstring(&mut out, &fragment.script);
                write_wstring(&mut out, &fragment.function);
            }
            out.extend_from_slice(&(scene.phases.len() as u16).to_le_bytes());
            for phase in &scene.phases {
                out.push(phase.flag);
                out.extend_from_slice(&phase.phase_index.to_le_bytes());
                out.push(phase.marker);
                write_wstring(&mut out, &phase.script);
                write_wstring(&mut out, &phase.function);
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

fn write_object(out: &mut Vec<u8>, o: ObjectRef, object_format: i16) {
    if object_format == 1 {
        out.extend_from_slice(&o.form_id.to_le_bytes());
        out.extend_from_slice(&o.alias.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
    } else {
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&o.alias.to_le_bytes());
        out.extend_from_slice(&o.form_id.to_le_bytes());
    }
}

fn write_scripts(out: &mut Vec<u8>, scripts: &[ScriptEntry], object_format: i16) {
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
                PropertyValue::Object(o) => write_object(out, *o, object_format),
                PropertyValue::Str(s) => write_wstring(out, s),
                PropertyValue::Int(i) => out.extend_from_slice(&i.to_le_bytes()),
                PropertyValue::Float(f) => out.extend_from_slice(&f.to_le_bytes()),
                PropertyValue::Bool(b) => out.push(u8::from(*b)),
                PropertyValue::Objects(v) => {
                    out.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    for o in v {
                        write_object(out, *o, object_format);
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

    const PACK: Tag = Tag::new(b"PACK");

    /// Bytes as xEdit's `wbVMADFragmentedPACK` would author them: no scripts,
    /// then the "Script Fragments" struct (extra bind data version, flags,
    /// file name, one fragment per set flag bit, in `OnBegin`, `OnEnd`,
    /// `OnChange` order).
    fn authored_pack_bytes(flags: u8, fragments: &[(u8, &str, &str)]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&5i16.to_le_bytes());
        bytes.extend_from_slice(&2i16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.push(2);
        bytes.push(flags);
        write_wstring(&mut bytes, "TIF__000A0001");
        for (marker, script, function) in fragments {
            bytes.push(*marker);
            write_wstring(&mut bytes, script);
            write_wstring(&mut bytes, function);
        }
        bytes
    }

    #[test]
    fn fmt_03_pack_begin_and_end_fragments_reproduce_authored_bytes() {
        for (flags, function, begin) in [(0b001, "Fragment_0", true), (0b010, "Fragment_1", false)]
        {
            let bytes = authored_pack_bytes(flags, &[(2, "TIF__000A0001", function)]);
            let v = Vmad::parse(&bytes, PACK).unwrap();
            let frags = v.pack_fragments.as_ref().unwrap();
            let actual = if begin { &frags.begin } else { &frags.end };
            assert_eq!(actual.as_ref().unwrap().function, function);
            if begin {
                assert!(frags.end.is_none());
            } else {
                assert!(frags.begin.is_none());
            }
            assert!(frags.change.is_none());
            assert_eq!(v.write(PACK), bytes);
        }
    }

    #[test]
    fn fmt_03_pack_change_fragment_reproduces_authored_bytes() {
        let bytes = authored_pack_bytes(0b100, &[(2, "TIF__000A0001", "Fragment_2")]);
        let v = Vmad::parse(&bytes, PACK).unwrap();
        let frags = v.pack_fragments.as_ref().unwrap();
        assert!(frags.begin.is_none());
        assert!(frags.end.is_none());
        assert_eq!(frags.change.as_ref().unwrap().function, "Fragment_2");
        assert_eq!(v.write(PACK), bytes);
    }

    #[test]
    fn fmt_03_pack_single_fragment_flags_reproduce_authored_bytes() {
        for (flags, function, expected_index) in [
            (0b001, "Fragment_0", 0),
            (0b010, "Fragment_1", 1),
            (0b100, "Fragment_2", 2),
        ] {
            let bytes = authored_pack_bytes(flags, &[(2, "TIF__000A0001", function)]);
            let parsed = Vmad::parse(&bytes, PACK).unwrap();
            let fragments = parsed.pack_fragments.as_ref().unwrap();
            assert_eq!(fragments.begin.is_some(), expected_index == 0);
            assert_eq!(fragments.end.is_some(), expected_index == 1);
            assert_eq!(fragments.change.is_some(), expected_index == 2);
            let actual = match expected_index {
                0 => &fragments.begin,
                1 => &fragments.end,
                _ => &fragments.change,
            };
            assert_eq!(actual.as_ref().unwrap().function, function);
            assert_eq!(parsed.write(PACK), bytes);
        }
    }

    #[test]
    fn fmt_03_pack_begin_end_and_change_together_reproduce_authored_bytes() {
        let bytes = authored_pack_bytes(
            0b111,
            &[
                (2, "TIF__000A0001", "Fragment_0"),
                (2, "TIF__000A0001", "Fragment_1"),
                (2, "TIF__000A0001", "Fragment_2"),
            ],
        );
        let v = Vmad::parse(&bytes, PACK).unwrap();
        let frags = v.pack_fragments.as_ref().unwrap();
        assert_eq!(frags.begin.as_ref().unwrap().function, "Fragment_0");
        assert_eq!(frags.end.as_ref().unwrap().function, "Fragment_1");
        assert_eq!(frags.change.as_ref().unwrap().function, "Fragment_2");
        assert_eq!(v.write(PACK), bytes);
    }

    #[test]
    fn fmt_03_pack_fragments_reject_unsupported_flags_and_short_input() {
        let good = authored_pack_bytes(
            0b111,
            &[
                (2, "TIF__000A0001", "Fragment_0"),
                (2, "TIF__000A0001", "Fragment_1"),
                (2, "TIF__000A0001", "Fragment_2"),
            ],
        );
        let mut bad_flags = good.clone();
        // version i16, object format i16, script count u16, extra bind version u8, then flags.
        let flags_at = 2 + 2 + 2 + 1;
        bad_flags[flags_at] = 0b1000;
        assert!(Vmad::parse(&bad_flags, PACK).is_err());
        for cut in [7, 9, 15, 25, good.len() - 1] {
            assert!(
                Vmad::parse(&good[..cut], PACK).is_err(),
                "cut at {cut} should fail"
            );
        }
    }
}
