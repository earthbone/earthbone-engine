// SPDX-License-Identifier: GPL-3.0-or-later

//! A packfile builder. Objects are allocated in a data section with their
//! pointer fields left zero, and the builder records the fixups that make
//! them real. `finish` lays out the header, the three sections and the fixup
//! tables the way the 2010 writer does, so the result reads back through
//! [`crate::Packfile`] and, as far as the layout goes, through Havok.
//!
//! The first object allocated is the file's contents (its root).

use std::collections::BTreeMap;

use crate::packfile::{HEADER_SIZE, MAGIC0, MAGIC1, SECTION_HEADER_SIZE};
use crate::QsTransform;

/// Signatures for the classes the tests and the reader care about. A class
/// that is not listed gets signature 0, which the reader never checks.
const SIGNATURES: &[(&str, u32)] = &[
    ("hkClass", 0x75585ef6),
    ("hkClassMember", 0x5c7ea4c2),
    ("hkClassEnum", 0x8a3609cf),
    ("hkClassEnumItem", 0xce6f8a6c),
    ("hkRootLevelContainer", 0x2772c11e),
    ("hkaAnimationContainer", 0x8dc20333),
    ("hkaSkeleton", 0x366e8220),
    ("hkaAnimationBinding", 0x66eac971),
    ("hkaSplineCompressedAnimation", 0x792ee0bb),
    ("hkbBehaviorGraph", 0xb1218f86),
    ("hkbBehaviorGraphData", 0x095aca5d),
    ("hkbBehaviorGraphStringData", 0xc713064e),
    ("hkbVariableValueSet", 0x27812d8d),
    ("hkbStateMachine", 0x816c1dcb),
    ("hkbStateMachineStateInfo", 0x0ed7f9d0),
    ("hkbStateMachineTransitionInfoArray", 0xe397b11e),
    ("hkbStateMachineEventPropertyArray", 0xb07b4388),
    ("hkbClipGenerator", 0x333b85b9),
    ("hkbClipTriggerArray", 0x59c23a0f),
    ("hkbBlenderGenerator", 0x22df7147),
    ("hkbBlenderGeneratorChild", 0xe2b384b0),
    ("hkbBlendingTransitionEffect", 0xfd8584fe),
    ("hkbVariableBindingSet", 0x338ad4ff),
    ("hkbManualSelectorGenerator", 0xd932fab8),
    ("hkbModifierGenerator", 0x1f81fae6),
    ("hkbModifierList", 0xa4180ca1),
    ("BSGetTimeStepModifier", 0xbda33bfe),
    ("BSLimbIKModifier", 0x8ea971e5),
    ("hkbCharacterData", 0x300d6808),
    ("hkbCharacterStringData", 0x655b42bc),
    ("hkbProjectData", 0x13a39ba7),
    ("hkbProjectStringData", 0x076ad60a),
];

fn signature_of(class: &str) -> u32 {
    SIGNATURES
        .iter()
        .find(|(c, _)| *c == class)
        .map(|(_, s)| *s)
        .unwrap_or(0)
}

fn align_up(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}

#[derive(Default)]
pub struct PackfileBuilder {
    is_32bit: bool,
    data: Vec<u8>,
    local: Vec<(u32, u32)>,
    global: Vec<(u32, u32)>,
    /// Object offset to class name, in allocation order.
    virtual_: Vec<(u32, String)>,
    classes: Vec<String>,
}

impl PackfileBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a Skyrim LE packfile with four-byte pointers and array headers.
    pub fn new_32bit() -> Self {
        Self {
            is_32bit: true,
            ..Self::default()
        }
    }

    fn pointer_size(&self) -> u32 {
        if self.is_32bit {
            4
        } else {
            8
        }
    }

    fn alloc(&mut self, size: usize, align: usize) -> u32 {
        let at = align_up(self.data.len(), align);
        self.data.resize(at + size, 0);
        at as u32
    }

    /// Allocate an object of a class. The first object is the root.
    pub fn object(&mut self, class: &str, size: usize) -> u32 {
        let at = self.alloc(size.max(1), 16);
        if !self.classes.iter().any(|c| c == class) {
            self.classes.push(class.to_string());
        }
        self.virtual_.push((at, class.to_string()));
        at
    }

    /// An embedded struct or array payload with no class of its own.
    pub fn blob(&mut self, bytes: &[u8]) -> u32 {
        let at = self.alloc(bytes.len().max(1), 16);
        self.data[at as usize..at as usize + bytes.len()].copy_from_slice(bytes);
        at
    }

    pub fn set_bytes(&mut self, obj: u32, off: u32, bytes: &[u8]) {
        let at = obj as usize + off as usize;
        assert!(at + bytes.len() <= self.data.len(), "write past object");
        self.data[at..at + bytes.len()].copy_from_slice(bytes);
    }
    pub fn set_u8(&mut self, obj: u32, off: u32, v: u8) {
        self.set_bytes(obj, off, &[v]);
    }
    pub fn set_i8(&mut self, obj: u32, off: u32, v: i8) {
        self.set_bytes(obj, off, &[v as u8]);
    }
    pub fn set_bool(&mut self, obj: u32, off: u32, v: bool) {
        self.set_u8(obj, off, v as u8);
    }
    pub fn set_u16(&mut self, obj: u32, off: u32, v: u16) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_i16(&mut self, obj: u32, off: u32, v: i16) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_u32(&mut self, obj: u32, off: u32, v: u32) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_i32(&mut self, obj: u32, off: u32, v: i32) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_u64(&mut self, obj: u32, off: u32, v: u64) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_f32(&mut self, obj: u32, off: u32, v: f32) {
        self.set_bytes(obj, off, &v.to_le_bytes());
    }
    pub fn set_vec4(&mut self, obj: u32, off: u32, v: [f32; 4]) {
        for (i, x) in v.iter().enumerate() {
            self.set_f32(obj, off + 4 * i as u32, *x);
        }
    }
    pub fn set_qs_transform(&mut self, obj: u32, off: u32, t: &QsTransform) {
        self.set_vec4(obj, off, t.translation);
        self.set_vec4(obj, off + 16, t.rotation);
        self.set_vec4(obj, off + 32, t.scale);
    }

    /// Point a pointer field at another object.
    pub fn ptr(&mut self, obj: u32, off: u32, target: u32) {
        self.global.push((obj + off, target));
    }

    /// Fill an `hkStringPtr` field.
    pub fn string(&mut self, obj: u32, off: u32, s: &str) {
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        let at = self.blob(&bytes);
        self.local.push((obj + off, at));
    }

    /// Fill an `hkArray` field with raw element bytes and a count.
    pub fn array_bytes(&mut self, obj: u32, off: u32, bytes: &[u8], count: usize) {
        self.set_i32(obj, off + self.pointer_size(), count as i32);
        self.set_i32(
            obj,
            off + self.pointer_size() + 4,
            (count as i32) | i32::MIN,
        );
        if count > 0 {
            let at = self.blob(bytes);
            self.local.push((obj + off, at));
        }
    }

    /// Fill an `hkArray<T*>` field, one global fixup per element.
    pub fn array_ptrs(&mut self, obj: u32, off: u32, targets: &[u32]) {
        self.set_i32(obj, off + self.pointer_size(), targets.len() as i32);
        self.set_i32(
            obj,
            off + self.pointer_size() + 4,
            (targets.len() as i32) | i32::MIN,
        );
        if targets.is_empty() {
            return;
        }
        let at = self.blob(&vec![0u8; self.pointer_size() as usize * targets.len()]);
        self.local.push((obj + off, at));
        for (i, t) in targets.iter().enumerate() {
            self.global.push((at + self.pointer_size() * i as u32, *t));
        }
    }

    /// Fill an `hkArray<hkStringPtr>` field.
    pub fn array_strings(&mut self, obj: u32, off: u32, strings: &[&str]) {
        self.set_i32(obj, off + self.pointer_size(), strings.len() as i32);
        self.set_i32(
            obj,
            off + self.pointer_size() + 4,
            (strings.len() as i32) | i32::MIN,
        );
        if strings.is_empty() {
            return;
        }
        let at = self.blob(&vec![0u8; self.pointer_size() as usize * strings.len()]);
        self.local.push((obj + off, at));
        for (i, s) in strings.iter().enumerate() {
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            let sat = self.blob(&bytes);
            self.local.push((at + self.pointer_size() * i as u32, sat));
        }
    }

    /// Allocate an array of `count` structs of `stride` bytes for a field
    /// and return the offset of element 0, so the caller can fill them.
    pub fn array_structs(&mut self, obj: u32, off: u32, stride: usize, count: usize) -> u32 {
        self.set_i32(obj, off + self.pointer_size(), count as i32);
        self.set_i32(
            obj,
            off + self.pointer_size() + 4,
            (count as i32) | i32::MIN,
        );
        if count == 0 {
            return 0;
        }
        let at = self.blob(&vec![0u8; stride * count]);
        self.local.push((obj + off, at));
        at
    }

    pub fn array_i16(&mut self, obj: u32, off: u32, v: &[i16]) {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.array_bytes(obj, off, &bytes, v.len());
    }
    pub fn array_i32(&mut self, obj: u32, off: u32, v: &[i32]) {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.array_bytes(obj, off, &bytes, v.len());
    }
    pub fn array_u32(&mut self, obj: u32, off: u32, v: &[u32]) {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.array_bytes(obj, off, &bytes, v.len());
    }
    pub fn array_f32(&mut self, obj: u32, off: u32, v: &[f32]) {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.array_bytes(obj, off, &bytes, v.len());
    }
    pub fn array_qs_transform(&mut self, obj: u32, off: u32, v: &[QsTransform]) {
        let mut bytes = Vec::with_capacity(48 * v.len());
        for t in v {
            for x in t.translation.iter().chain(&t.rotation).chain(&t.scale) {
                bytes.extend_from_slice(&x.to_le_bytes());
            }
        }
        self.array_bytes(obj, off, &bytes, v.len());
    }

    /// Lay out the file.
    pub fn finish(mut self) -> Vec<u8> {
        // Class names: the four meta classes first, then every class used.
        let mut all_classes = vec![
            "hkClass".to_string(),
            "hkClassMember".to_string(),
            "hkClassEnum".to_string(),
            "hkClassEnumItem".to_string(),
        ];
        for c in &self.classes {
            if !all_classes.contains(c) {
                all_classes.push(c.clone());
            }
        }
        let mut class_bytes = Vec::new();
        let mut name_offsets: BTreeMap<String, u32> = BTreeMap::new();
        for c in &all_classes {
            class_bytes.extend_from_slice(&signature_of(c).to_le_bytes());
            class_bytes.push(0x09);
            name_offsets.insert(c.clone(), class_bytes.len() as u32);
            class_bytes.extend_from_slice(c.as_bytes());
            class_bytes.push(0);
        }
        let class_len = align_up(class_bytes.len(), 16);
        class_bytes.resize(class_len, 0xFF);

        // Data section: objects, then the three fixup tables, each padded
        // to 16 with 0xFF.
        let data_len = align_up(self.data.len(), 16);
        self.data.resize(data_len, 0);
        let mut data = std::mem::take(&mut self.data);
        let local_off = data.len();
        for (src, dst) in &self.local {
            data.extend_from_slice(&src.to_le_bytes());
            data.extend_from_slice(&dst.to_le_bytes());
        }
        data.resize(align_up(data.len(), 16), 0xFF);
        let global_off = data.len();
        for (src, dst) in &self.global {
            data.extend_from_slice(&src.to_le_bytes());
            data.extend_from_slice(&2u32.to_le_bytes());
            data.extend_from_slice(&dst.to_le_bytes());
        }
        data.resize(align_up(data.len(), 16), 0xFF);
        let virtual_off = data.len();
        for (src, class) in &self.virtual_ {
            data.extend_from_slice(&src.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&name_offsets[class].to_le_bytes());
        }
        data.resize(align_up(data.len(), 16), 0xFF);
        let end_off = data.len();

        let class_start = (HEADER_SIZE + 3 * SECTION_HEADER_SIZE) as u32;
        let data_start = class_start + class_len as u32;
        let root_class_offset = self
            .virtual_
            .first()
            .map(|(_, c)| name_offsets[c])
            .unwrap_or(0);

        let mut out = Vec::with_capacity(data_start as usize + data.len());
        out.extend_from_slice(&MAGIC0.to_le_bytes());
        out.extend_from_slice(&MAGIC1.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes()); // user tag
        out.extend_from_slice(&8i32.to_le_bytes()); // file version
        out.extend_from_slice(&[self.pointer_size() as u8, 1, 0, 1]); // ptr size, little endian, padding, base class
        out.extend_from_slice(&3i32.to_le_bytes());
        out.extend_from_slice(&2i32.to_le_bytes()); // contents section
        out.extend_from_slice(&0i32.to_le_bytes()); // contents offset
        out.extend_from_slice(&0i32.to_le_bytes()); // contents class name section
        out.extend_from_slice(&(root_class_offset as i32).to_le_bytes());
        let mut ver = *b"hk_2010.2.0-r1\0\xFF";
        ver[14] = 0;
        out.extend_from_slice(&ver);
        out.extend_from_slice(&0i32.to_le_bytes()); // flags
        out.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // max predicate, section offset

        let section = |out: &mut Vec<u8>, tag: &str, start: u32, offs: [u32; 6]| {
            let mut t = [0u8; 20];
            t[..tag.len()].copy_from_slice(tag.as_bytes());
            t[19] = 0xFF;
            out.extend_from_slice(&t);
            out.extend_from_slice(&start.to_le_bytes());
            for o in offs {
                out.extend_from_slice(&o.to_le_bytes());
            }
        };
        let cl = class_len as u32;
        section(&mut out, "__classnames__", class_start, [cl; 6]);
        section(&mut out, "__types__", data_start, [0; 6]);
        section(
            &mut out,
            "__data__",
            data_start,
            [
                local_off as u32,
                global_off as u32,
                virtual_off as u32,
                end_off as u32,
                end_off as u32,
                end_off as u32,
            ],
        );
        debug_assert_eq!(out.len() as u32, class_start);
        out.extend_from_slice(&class_bytes);
        out.extend_from_slice(&data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Packfile;

    #[test]
    fn the_header_and_sections_read_back() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        b.set_i32(root, 12, i32::MIN);
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        assert_eq!(pf.header.version_string, "hk_2010.2.0-r1");
        assert_eq!(pf.header.pointer_size, 8);
        assert_eq!(pf.sections.len(), 3);
        assert_eq!(pf.sections[0].absolute_data_start, 208);
        assert_eq!(
            pf.objects.get(&0).map(String::as_str),
            Some("hkRootLevelContainer")
        );
        assert!(pf.class_names.values().any(|c| c == "hkClassEnumItem"));
    }
}
