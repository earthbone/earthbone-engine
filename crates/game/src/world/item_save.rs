// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW distinct-item extension. Legacy totals are checked against stack counts.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use soe_esm::GlobalFormId;
use soe_papyrus::{vm::state::Writer, VmError};

use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{RefState, World};
use crate::inventory::{Extra, Field, Inventory, Stack};

fn bad(reason: impl std::fmt::Display) -> VmError {
    VmError::State(format!("inventory state: {reason}"))
}

fn write_fields(w: &mut Writer, fields: &[Field]) {
    w.len(fields.len());
    for field in fields {
        match field {
            Field::Byte(v) => {
                w.u8(0);
                w.u8(*v);
            }
            Field::Short(v) => {
                w.u8(1);
                w.u16(*v);
            }
            Field::Word(v) => {
                w.u8(2);
                w.u32(*v);
            }
            Field::FloatBits(v) => {
                w.u8(3);
                w.u32(*v);
            }
            Field::Form(v) => {
                w.u8(4);
                write_opt_form_id(w, *v);
            }
            Field::Text(v) => {
                w.u8(5);
                w.bytes(v);
            }
            Field::Bytes(v) => {
                w.u8(6);
                w.bytes(v);
            }
            Field::Array(rows) => {
                w.u8(7);
                w.len(rows.len());
                for row in rows {
                    write_fields(w, row);
                }
            }
            Field::Group(fields) => {
                w.u8(8);
                write_fields(w, fields);
            }
        }
    }
}

fn read_fields(r: &mut FormReader<'_, '_>, depth: usize) -> Result<Vec<Field>, VmError> {
    if depth > 128 {
        return Err(bad("extra-data nesting exceeds 128 levels"));
    }
    let n = r.len()?;
    let mut fields = Vec::new();
    for _ in 0..n {
        fields.push(match r.u8()? {
            0 => Field::Byte(r.u8()?),
            1 => Field::Short(r.u16()?),
            2 => Field::Word(r.u32()?),
            3 => Field::FloatBits(r.u32()?),
            4 => Field::Form(read_opt_form_id(r)?),
            5 => Field::Text(r.bytes()?.to_vec()),
            6 => Field::Bytes(r.bytes()?.to_vec()),
            7 => {
                let n = r.len()?;
                let mut rows = Vec::new();
                for _ in 0..n {
                    rows.push(read_fields(r, depth + 1)?);
                }
                Field::Array(rows)
            }
            8 => Field::Group(read_fields(r, depth + 1)?),
            _ => return Err(bad("unknown extra-data field tag")),
        });
    }
    Ok(fields)
}

pub(super) fn write_stacks(w: &mut Writer, inventory: &Inventory) {
    let items: Vec<_> = inventory
        .iter()
        .filter(|(id, _)| !inventory.stacks(**id).is_empty())
        .collect();
    w.len(items.len());
    for (id, _) in items {
        write_form_id(w, *id);
        let stacks = inventory.stacks(*id);
        w.len(stacks.len());
        for stack in stacks {
            w.i32(stack.count());
            w.len(stack.extras().len());
            for extra in stack.extras() {
                w.u8(extra.kind);
                write_fields(w, &extra.fields);
            }
        }
    }
}

fn read_stacks(r: &mut FormReader<'_, '_>, inventory: &mut Inventory) -> Result<(), VmError> {
    let n = r.len()?;
    let mut seen = BTreeSet::new();
    for _ in 0..n {
        let id = read_form_id(r)?;
        if !seen.insert(id) || !inventory.contains(id) {
            return Err(bad("duplicate or absent inventory item"));
        }
        let n = r.len()?;
        if n == 0 {
            return Err(bad("empty item extension"));
        }
        let mut stacks = Vec::new();
        for _ in 0..n {
            let count = r.i32()?;
            let n = r.len()?;
            let mut extras = Vec::new();
            for _ in 0..n {
                extras.push(Extra {
                    kind: r.u8()?,
                    fields: read_fields(r, 0)?,
                });
            }
            stacks.push(Stack::new(count, extras).map_err(bad)?);
        }
        inventory
            .restore(id, inventory.count(id), stacks)
            .map_err(bad)?;
    }
    Ok(())
}

impl World {
    pub(super) fn write_item_stacks(&self, w: &mut Writer) {
        let refs: BTreeMap<_, _> = self
            .refs
            .iter()
            .filter(|(_, state)| {
                state
                    .inventory
                    .iter()
                    .any(|(id, _)| !state.inventory.stacks(*id).is_empty())
            })
            .collect();
        w.len(refs.len());
        for (id, state) in refs {
            write_form_id(w, *id);
            write_stacks(w, &state.inventory);
        }
    }
}

pub(super) fn read_item_stacks(
    r: &mut FormReader<'_, '_>,
    refs: &mut HashMap<GlobalFormId, RefState>,
) -> Result<(), VmError> {
    let n = r.len()?;
    let mut seen = BTreeSet::new();
    for _ in 0..n {
        let id = read_form_id(r)?;
        if !seen.insert(id) {
            return Err(bad("duplicate reference extension"));
        }
        let state = refs
            .get_mut(&id)
            .ok_or_else(|| bad("unknown reference extension"))?;
        read_stacks(r, &mut state.inventory)?;
    }
    Ok(())
}
