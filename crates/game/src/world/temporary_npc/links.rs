// SPDX-License-Identifier: GPL-3.0-or-later

//! Relocate the FormID fields in NPC components using xEdit's subrecord layouts.
//! Opaque bytes and floating-point fields are never treated as references.

use soe_esm::{plugin::Subrecord, Error, GlobalFormId};

use super::{invalid, Builder, T};

fn word(data: &[u8], at: usize) -> Result<u32, Error> {
    let bytes = data
        .get(at..at + 4)
        .ok_or_else(|| invalid("truncated NPC link data"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

impl Builder<'_> {
    fn link(&mut self, source: GlobalFormId, raw: u32) -> Result<u32, Error> {
        let id = self.world.resolve_from(source, raw);
        if raw != 0 && id.is_none() {
            return Err(invalid("unresolved NPC source link"));
        }
        self.value.link(&mut self.used, id).map_err(invalid)
    }

    fn at(&mut self, source: GlobalFormId, data: &mut [u8], at: usize) -> Result<(), Error> {
        let raw = self.link(source, word(data, at)?)?;
        data[at..at + 4].copy_from_slice(&raw.to_le_bytes());
        Ok(())
    }

    pub(super) fn relocate(
        &mut self,
        source: GlobalFormId,
        sub: &mut Subrecord,
    ) -> Result<(), Error> {
        let data = &mut sub.data;
        match sub.tag.as_bytes() {
            b"INAM" | b"VTCK" | b"RNAM" | b"WNAM" | b"ANAM" | b"PNAM" | b"HCLF" | b"CSDI"
            | b"CSCR" | b"FTST" | b"CNAM" | b"CRIF" | b"SPLO" | b"ZNAM" | b"GNAM" | b"PKID"
            | b"DOFT" | b"SOFT" | b"SPOR" | b"OCOR" | b"GWOR" | b"ECOR" | b"DPLT" | b"ATKR" => {
                if data.len() != 4 {
                    return Err(invalid("invalid NPC link width"));
                }
                self.at(source, data, 0)?;
            }
            b"SNAM" | b"PRKR" | b"CNTO" => {
                if data.len() != 8 {
                    return Err(invalid("invalid NPC entry width"));
                }
                self.at(source, data, 0)?;
            }
            b"KWDA" => {
                if !data.len().is_multiple_of(4) {
                    return Err(invalid("invalid NPC keyword width"));
                }
                for at in (0..data.len()).step_by(4) {
                    self.at(source, data, at)?;
                }
            }
            b"ATKD" => {
                if data.len() != 44 {
                    return Err(invalid("invalid NPC attack width"));
                }
                self.at(source, data, 8)?;
                self.at(source, data, 28)?;
            }
            b"DSTD" => {
                if data.len() != 20 {
                    return Err(invalid("invalid NPC destruction width"));
                }
                self.at(source, data, 8)?;
                self.at(source, data, 12)?;
            }
            b"COED" => {
                if data.len() != 12 {
                    return Err(invalid("invalid NPC ownership width"));
                }
                let raw = word(data, 0)?;
                let owner = self.world.resolve_from(source, raw);
                if let Some(owner) = owner {
                    match self.world.tag_of(owner) {
                        Some(tag) if tag == T(b"NPC_") => self.at(source, data, 4)?,
                        Some(tag) if tag == T(b"FACT") => {}
                        _ => return Err(invalid("unknown NPC inventory owner type")),
                    }
                }
                self.at(source, data, 0)?;
            }
            b"DMDS" => {
                let count = word(data, 0)? as usize;
                if count > data.len().saturating_sub(4) / 12 {
                    return Err(invalid("NPC alternate texture count exceeds data"));
                }
                let mut at = 4;
                for _ in 0..count {
                    let len = word(data, at)? as usize;
                    at = at
                        .checked_add(4 + len)
                        .ok_or_else(|| invalid("NPC texture name overflow"))?;
                    self.at(source, data, at)?;
                    at += 8;
                }
                if at != data.len() {
                    return Err(invalid("invalid NPC alternate texture tail"));
                }
            }
            b"VMAD" => {
                use crate::vmad::{PropertyValue, Vmad};
                let mut vmad = Vmad::parse(data, T(b"NPC_"))?;
                if !vmad.trailing.is_empty() {
                    return Err(invalid("unparsed NPC VMAD tail"));
                }
                for script in &mut vmad.scripts {
                    for property in &mut script.properties {
                        match &mut property.value {
                            PropertyValue::Object(value) => {
                                value.form_id = self.link(source, value.form_id)?;
                            }
                            PropertyValue::Objects(values) => {
                                for value in values {
                                    value.form_id = self.link(source, value.form_id)?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                *data = vmad.write(T(b"NPC_"));
            }
            _ => {}
        }
        Ok(())
    }
}
