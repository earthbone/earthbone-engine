// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte serialization for the Skyrim SE collision graph emitted by
//! `soe-physics`.

use std::collections::HashMap;

use crate::blocks::{Block, Node};
use crate::error::{Error, Result};
use crate::header::{BS_SKYRIM_SE, VERSION_20_2_0_7};
use crate::types::BlockRef;
use crate::NifFile;

const HEADER_LINE: &[u8] = b"Gamebryo File Format, Version 20.2.0.7\n";

/// Serialize a Skyrim SE NIF containing nodes and opaque collision blocks.
///
/// Opaque collision payloads are copied unchanged. Node references and roots
/// are validated against the in-memory block list before bytes are produced.
pub fn to_bytes(file: &NifFile) -> Result<Vec<u8>> {
    if file.header.version != VERSION_20_2_0_7
        || file.header.user_version != 12
        || file.header.bs_version != BS_SKYRIM_SE
    {
        return Err(Error::UnsupportedWrite(
            "only Skyrim SE 20.2.0.7 user version 12 files can be written".into(),
        ));
    }

    for (index, block) in file.blocks.iter().enumerate() {
        if !matches!(
            block,
            Block::Node(_) | Block::Collision { .. } | Block::Unknown { .. }
        ) {
            return Err(Error::UnsupportedWrite(format!(
                "block {index} ({}) is outside the collision writer subset",
                block.type_name()
            )));
        }
        if let Block::Node(node) = block {
            validate_node_refs(index, node, file.blocks.len())?;
        }
    }
    for root in &file.roots {
        validate_ref(file.blocks.len(), file.blocks.len(), *root)?;
    }

    let mut strings = file.header.strings.clone();
    let mut string_indices = HashMap::new();
    for (index, value) in strings.iter().enumerate() {
        string_indices.entry(value.clone()).or_insert(index as u32);
    }
    for block in &file.blocks {
        if let Block::Node(node) = block {
            if !node.av.net.name.is_empty() && !string_indices.contains_key(&node.av.net.name) {
                let index = u32::try_from(strings.len())
                    .map_err(|_| Error::UnsupportedWrite("too many strings".into()))?;
                string_indices.insert(node.av.net.name.clone(), index);
                strings.push(node.av.net.name.clone());
            }
        }
    }

    let block_types: Vec<String> = {
        let mut values = Vec::new();
        for block in &file.blocks {
            let name = match block {
                Block::Collision { type_name, .. } | Block::Unknown { type_name, .. } => {
                    type_name.as_str()
                }
                _ => block.type_name(),
            };
            if !values.iter().any(|existing| existing == name) {
                values.push(name.to_string());
            }
        }
        values
    };
    let type_indices: HashMap<&str, u16> = block_types
        .iter()
        .enumerate()
        .map(|(index, name)| {
            u16::try_from(index)
                .map(|index| (name.as_str(), index))
                .map_err(|_| Error::UnsupportedWrite("too many block types".into()))
        })
        .collect::<Result<_>>()?;

    let payloads: Vec<Vec<u8>> = file
        .blocks
        .iter()
        .map(|block| match block {
            Block::Node(node) => encode_node(node, &string_indices),
            Block::Collision { data, .. } | Block::Unknown { data, .. } => Ok(data.clone()),
            _ => unreachable!("block kinds were checked above"),
        })
        .collect::<Result<_>>()?;

    let mut out = Vec::new();
    out.extend_from_slice(HEADER_LINE);
    put_u32(&mut out, VERSION_20_2_0_7);
    out.push(1);
    put_u32(&mut out, 12);
    put_count(&mut out, file.blocks.len(), "too many blocks")?;
    put_u32(&mut out, BS_SKYRIM_SE);
    put_export_string(&mut out, &file.header.author)?;
    put_export_string(&mut out, &file.header.process_script)?;
    put_export_string(&mut out, &file.header.export_script)?;
    put_u16_count(&mut out, block_types.len(), "too many block types")?;
    for name in &block_types {
        put_sized_string(&mut out, name)?;
    }
    for block in &file.blocks {
        let name = match block {
            Block::Collision { type_name, .. } | Block::Unknown { type_name, .. } => {
                type_name.as_str()
            }
            _ => block.type_name(),
        };
        put_u16(&mut out, type_indices[name]);
    }
    for payload in &payloads {
        put_count(&mut out, payload.len(), "NIF block exceeds u32 size")?;
    }
    put_count(&mut out, strings.len(), "too many strings")?;
    let max_string = strings
        .iter()
        .map(|value| encoded_string(value).map(|b| b.len()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .unwrap_or(0);
    put_count(&mut out, max_string, "string exceeds u32 size")?;
    for value in &strings {
        put_sized_string(&mut out, value)?;
    }
    put_count(&mut out, file.header.groups.len(), "too many groups")?;
    for group in &file.header.groups {
        put_u32(&mut out, *group);
    }
    for payload in payloads {
        out.extend_from_slice(&payload);
    }
    put_count(&mut out, file.roots.len(), "too many roots")?;
    for root in &file.roots {
        put_i32(&mut out, root.0);
    }
    Ok(out)
}

fn validate_node_refs(index: usize, node: &Node, block_count: usize) -> Result<()> {
    for reference in node
        .av
        .net
        .extra_data
        .iter()
        .copied()
        .chain([node.av.net.controller, node.av.collision])
        .chain(node.children.iter().copied())
        .chain(node.effects.iter().copied())
    {
        validate_ref(index, block_count, reference)?;
    }
    Ok(())
}

fn validate_ref(block: usize, block_count: usize, reference: BlockRef) -> Result<()> {
    if let Some(index) = reference.get() {
        if index >= block_count {
            return Err(Error::BadRef {
                block,
                reference: reference.0,
            });
        }
    }
    Ok(())
}

fn encode_node(node: &Node, strings: &HashMap<String, u32>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    put_u32(
        &mut out,
        if node.av.net.name.is_empty() {
            u32::MAX
        } else {
            *strings.get(&node.av.net.name).ok_or_else(|| {
                Error::UnsupportedWrite("node name is absent from string table".into())
            })?
        },
    );
    put_refs(&mut out, &node.av.net.extra_data)?;
    put_i32(&mut out, node.av.net.controller.0);
    put_u32(&mut out, node.av.flags);
    let translation = node.av.transform.translation;
    put_f32(&mut out, translation.x);
    put_f32(&mut out, translation.y);
    put_f32(&mut out, translation.z);
    for column in 0..3 {
        for row in 0..3 {
            put_f32(&mut out, node.av.transform.rotation.rows[row][column]);
        }
    }
    put_f32(&mut out, node.av.transform.scale);
    put_i32(&mut out, node.av.collision.0);
    put_refs(&mut out, &node.children)?;
    put_refs(&mut out, &node.effects)?;
    Ok(out)
}

fn put_refs(out: &mut Vec<u8>, refs: &[BlockRef]) -> Result<()> {
    put_count(out, refs.len(), "too many block references")?;
    for reference in refs {
        put_i32(out, reference.0);
    }
    Ok(())
}

fn encoded_string(value: &str) -> Result<Vec<u8>> {
    let (bytes, _, had_errors) = encoding_rs::WINDOWS_1252.encode(value);
    if had_errors {
        return Err(Error::UnsupportedWrite(
            "string contains characters outside Windows-1252".into(),
        ));
    }
    Ok(bytes.into_owned())
}

fn put_export_string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = encoded_string(value)?;
    let length = bytes
        .len()
        .checked_add(1)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| Error::UnsupportedWrite("export string exceeds 254 bytes".into()))?;
    out.push(length);
    out.extend_from_slice(&bytes);
    out.push(0);
    Ok(())
}

fn put_sized_string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = encoded_string(value)?;
    put_count(out, bytes.len(), "string exceeds u32 size")?;
    out.extend_from_slice(&bytes);
    Ok(())
}

fn put_count(out: &mut Vec<u8>, count: usize, message: &str) -> Result<()> {
    let count = u32::try_from(count).map_err(|_| Error::UnsupportedWrite(message.into()))?;
    put_u32(out, count);
    Ok(())
}

fn put_u16_count(out: &mut Vec<u8>, count: usize, message: &str) -> Result<()> {
    let count = u16::try_from(count).map_err(|_| Error::UnsupportedWrite(message.into()))?;
    put_u16(out, count);
    Ok(())
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}
