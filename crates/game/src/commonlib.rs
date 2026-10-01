// SPDX-License-Identifier: GPL-3.0-or-later

//! CommonLib source-level views over the live form and reference state.
//!
//! The exposed names follow the local CommonLibSSE-NG headers. The MIT shim
//! owns the declarations, and this module resolves them through `World`.

use soe_commonlib_shim::form::{FormId, FormType};
use soe_commonlib_shim::world::{
    IFormFactory, InventoryChanges, InventoryEntryData, ItemList, LookupReferenceByHandle,
    ReferenceKind,
};
use soe_esm::{GlobalFormId, Tag};

use crate::World;

/// A requested form factory signature is absent from the loaded form table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The form is not a placed reference in the current world.
    #[error("reference form does not exist: {0:?}")]
    Reference(GlobalFormId),
    /// The handle is not a runtime form identity in the current load order.
    #[error("reference handle does not resolve: {0:08X}")]
    Handle(u32),
    /// The loaded form table has no winner with this signature.
    #[error("no loaded forms use signature {0:?}")]
    Factory(FormType),
}

/// Describe a factory only when its signature has a loaded record.
///
/// # Errors
/// Returns [`Error::Factory`] when no winning form has the requested type.
pub fn form_factory(world: &World, signature: [u8; 4]) -> Result<IFormFactory, Error> {
    let tag = Tag::new(&signature);
    if world.order().winners_of(tag).next().is_none() {
        return Err(Error::Factory(FormType(signature)));
    }
    Ok(IFormFactory {
        name: String::from_utf8_lossy(&signature).into_owned(),
        form_type: FormType(signature),
    })
}

/// Read a reference's current inventory from the shared runtime state.
///
/// # Errors
/// Returns [`Error::Reference`] when the form is not a placed reference.
pub fn inventory_changes(
    world: &mut World,
    owner: GlobalFormId,
) -> Result<InventoryChanges, Error> {
    let runtime_owner = FormId(world.runtime_id(owner));
    let inventory = world
        .reference(owner)
        .ok_or(Error::Reference(owner))?
        .inventory
        .clone();
    let mut entries = Vec::new();
    let mut totals = Vec::new();
    for (item, total) in inventory.iter() {
        totals.push((FormId(world.runtime_id(*item)), total));
        let stacks = inventory.stacks(*item);
        let mut represented = 0_i32;
        for stack in stacks {
            represented = represented.saturating_add(stack.count());
            entries.push(InventoryEntryData {
                object: FormId(world.runtime_id(*item)),
                count_delta: stack.count(),
                extra_types: stack.extras().iter().map(|extra| extra.kind).collect(),
                worn: stack.worn(),
            });
        }
        let remainder = total.saturating_sub(represented);
        if remainder > 0 {
            entries.push(InventoryEntryData {
                object: FormId(world.runtime_id(*item)),
                count_delta: remainder,
                extra_types: Vec::new(),
                worn: false,
            });
        }
    }
    Ok(InventoryChanges {
        owner: runtime_owner,
        entries,
        totals,
    })
}

/// Project inventory entries into a menu-neutral item list.
///
/// # Errors
/// Returns [`Error::Reference`] when the owner is not a placed reference.
pub fn item_list(world: &mut World, owner: GlobalFormId) -> Result<ItemList, Error> {
    let changes = inventory_changes(world, owner)?;
    Ok(ItemList {
        items: changes.entries,
    })
}

/// Resolve a shim handle represented by a runtime FormID to live reference state.
///
/// # Errors
/// Returns [`Error::Reference`] when the handle is absent or names a non-reference form.
pub fn lookup_reference_by_handle(
    world: &mut World,
    handle: FormId,
) -> Result<LookupReferenceByHandle, Error> {
    let id = world
        .from_runtime_id(handle.0)
        .ok_or(Error::Handle(handle.0))?;
    let reference = world.reference(id).ok_or(Error::Reference(id))?;
    Ok(LookupReferenceByHandle {
        form_id: handle,
        kind: if reference.is_actor {
            ReferenceKind::Actor
        } else {
            ReferenceKind::Reference
        },
    })
}
