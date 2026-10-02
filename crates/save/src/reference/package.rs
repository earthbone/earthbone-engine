// SPDX-License-Identifier: GPL-3.0-or-later

//! Observed kind-18 Sequence and Stacked frames from the external 60-save corpus.
//! PACK tree names follow xEdit and UESP PACK. Unnamed child fields remain raw.
//! Public save readers do not define these bodies. See docs/save-package-data.md.

use super::fields::{array, fields, invalid, Field};
use crate::cursor::Cursor;
use crate::Result;

fn raw(r: &mut Cursor<'_>, n: usize) -> Result<Field> {
    Ok(Field::Bytes(r.take(n)?.to_vec()))
}

fn path_location(r: &mut Cursor<'_>) -> Result<Vec<Field>> {
    let mut v = vec![raw(r, 21)?];
    let world = r.ref_id()?;
    v.push(Field::Reference(world));
    if world.is_zero() {
        v.extend(fields(r, "r")?);
    } else {
        v.push(raw(r, 4)?);
    }
    v.extend(fields(r, "r")?);
    v.push(raw(r, 3)?);
    Ok(v)
}

fn path(r: &mut Cursor<'_>) -> Result<(Vec<Field>, u32)> {
    let token = r.u32()?;
    let mut v = vec![Field::Word(token)];
    if token == 0 {
        return Ok((v, 0));
    }
    let mode = r.u16()?;
    v.push(Field::Short(mode));
    if !matches!(mode, 1 | 2) {
        return Err(invalid(format!("unsupported Sequence path mode {mode}")));
    }
    let marker = r.u8()?;
    let kind = r.u32()?;
    if !matches!(kind, 0x330E_B0E3 | 0xEB5E_D874) {
        return Err(invalid(format!("unsupported Sequence request {kind:#x}")));
    }
    v.extend([Field::Byte(marker), Field::Word(kind)]);
    // CONTEXT: The first point shares its leading fields with the request tag.
    let mut first = vec![raw(r, 16)?];
    let world = r.ref_id()?;
    first.push(Field::Reference(world));
    if world.is_zero() {
        first.extend(fields(r, "r")?);
    } else {
        first.push(raw(r, 4)?);
    }
    first.extend(fields(r, "r")?);
    first.push(raw(r, 3)?);
    v.push(Field::Group(first));
    v.push(Field::Group(path_location(r)?));
    v.push(raw(r, 29)?);
    let extended = r.u32()?;
    v.push(Field::Word(extended));
    if extended == 1 {
        v.push(raw(r, 36)?);
    } else if extended != 0 {
        return Err(invalid("unsupported Sequence request extension"));
    }
    v.push(raw(r, if kind == 0x330E_B0E3 { 76 } else { 71 })?);
    v.push(Field::Array(array(r, true, 4, |r| fields(r, "rb"))?));
    Ok((v, kind))
}

fn location(r: &mut Cursor<'_>) -> Result<Vec<Field>> {
    let present = r.u8()?;
    let mut v = vec![Field::Byte(present)];
    if present == 0 {
        return Ok(v);
    }
    if present != 1 {
        return Err(invalid("unsupported Sequence location marker"));
    }
    let name = r.wstring()?;
    let point = match name.bytes() {
        b"LocRef" => false,
        b"LocPt" => true,
        _ => return Err(invalid(format!("unsupported Sequence location {name:?}"))),
    };
    v.push(Field::String(name));
    if point {
        v.push(raw(r, 12)?);
    }
    v.extend(fields(r, "r")?);
    v.push(raw(r, 12)?);
    Ok(v)
}

fn wander(r: &mut Cursor<'_>) -> Result<Vec<Field>> {
    let mut v = location(r)?;
    v.push(raw(r, 11)?);
    let (request, kind) = path(r)?;
    v.push(Field::Group(request));
    v.push(raw(r, 4)?);
    if kind == 0x330E_B0E3 {
        return Ok(v);
    }
    match kind {
        0xEB5E_D874 => {
            v.extend(fields(r, "w")?);
            v.push(raw(r, 5)?);
            let mut point = vec![raw(r, 16)?];
            let world = r.ref_id()?;
            point.push(Field::Reference(world));
            if world.is_zero() {
                point.extend(fields(r, "r")?);
            } else {
                point.push(raw(r, 4)?);
            }
            point.extend(fields(r, "r")?);
            point.push(raw(r, 6)?);
            v.push(Field::Group(point));
        }
        _ => return Err(invalid("unsupported Sequence Wander request")),
    }
    Ok(v)
}

fn sandbox(r: &mut Cursor<'_>) -> Result<Vec<Field>> {
    let mut v = vec![raw(r, 74)?];
    v.push(Field::Array(array(r, false, 15, |r| {
        Ok(vec![raw(r, 15)?])
    })?));
    v.push(Field::Group(location(r)?));
    v.push(raw(r, 1)?);
    let child = r.wstring()?;
    let empty = child.bytes().is_empty();
    if !empty && child.bytes() != b"Wander" {
        return Err(invalid(format!(
            "unsupported Sequence Sandbox child {child:?}"
        )));
    }
    v.push(Field::String(child));
    if empty {
        v.push(raw(r, 5)?);
        let (request, kind) = path(r)?;
        v.push(Field::Group(request));
        // CONTEXT: An absent request has no populated-request suffix byte.
        v.push(raw(r, if kind == 0 { 8 } else { 9 })?);
    } else {
        v.push(Field::Group(wander(r)?));
        v.push(raw(r, 17)?);
    }
    Ok(v)
}

pub(super) fn child(r: &mut Cursor<'_>, depth: usize) -> Result<Vec<Field>> {
    if depth >= 64 {
        return Err(invalid("Sequence procedure nesting exceeds 64"));
    }
    let name = r.wstring()?;
    let body = match name.bytes() {
        b"Follow" => {
            let mut v = fields(r, "r")?;
            v.push(raw(r, 16)?);
            let (request, _) = path(r)?;
            v.push(Field::Group(request));
            v.push(raw(r, 8)?);
            v
        }
        b"Sandbox" => sandbox(r)?,
        b"Simultaneous" => vec![Field::Array(array(r, false, 6, |r| {
            let mut v = fields(r, "w")?;
            v.push(Field::Group(child(r, depth + 1)?));
            Ok(v)
        })?)],
        b"Sequence" => sequence(r, depth + 1)?,
        b"Stacked" => stacked(r, depth + 1)?,
        b"SitSleep" => vec![raw(r, 3)?],
        b"Wait" => vec![raw(r, 4)?],
        _ => return Err(invalid(format!("unsupported Sequence child {name:?}"))),
    };
    Ok(vec![Field::String(name), Field::Group(body)])
}

pub(super) fn sequence(r: &mut Cursor<'_>, depth: usize) -> Result<Vec<Field>> {
    let mut v = vec![Field::Group(child(r, depth)?)];
    v.extend(fields(r, "w")?);
    Ok(v)
}

pub(super) fn stacked(r: &mut Cursor<'_>, depth: usize) -> Result<Vec<Field>> {
    sequence(r, depth).map_err(|error| invalid(format!("Stacked state: {error}")))
}
