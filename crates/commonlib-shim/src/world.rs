// SPDX-License-Identifier: MIT

//! CommonLib inventory and reference contracts backed by the game runtime.
//!
//! The type and method names follow the local CommonLibSSE-NG headers for
//! `IFormFactory`, inventory data and `LookupReferenceByHandle`. These are
//! source-level views, not ABI-compatible Skyrim objects.

use crate::form::{FormId, FormType};

/// An operation the Rust runtime cannot provide with the same contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// Runtime form allocation is not part of the immutable load-order API.
    FormCreation,
    /// Inventory writes must go through game inventory transactions.
    InventoryMutation,
    /// The runtime has no Scaleform menu selection state.
    ItemSelection,
    /// The runtime has no Scaleform menu to update.
    ItemListUpdate,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FormCreation => formatter.write_str("IFormFactory::Create is unsupported"),
            Self::InventoryMutation => {
                formatter.write_str("InventoryChanges mutation is unsupported")
            }
            Self::ItemSelection => formatter.write_str("ItemList selection is unsupported"),
            Self::ItemListUpdate => formatter.write_str("ItemList::Update is unsupported"),
        }
    }
}

impl std::error::Error for Unsupported {}

/// Read-only descriptor for a registered form factory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IFormFactory {
    /// Factory's source-facing name.
    pub name: String,
    /// Four-character record signature produced by the factory.
    pub form_type: FormType,
}

impl IFormFactory {
    /// Return the signature of forms this factory describes.
    #[must_use]
    pub const fn get_form_type(&self) -> FormType {
        self.form_type
    }

    /// Return this factory's source-facing name.
    #[must_use]
    pub fn get_form_name(&self) -> &str {
        &self.name
    }

    /// Form allocation is unavailable because forms come from the load order.
    ///
    /// # Errors
    /// Always returns [`Unsupported::FormCreation`].
    pub fn create(&self) -> Result<(), Unsupported> {
        Err(Unsupported::FormCreation)
    }
}

/// The runtime fields of one inventory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryEntryData {
    /// Resolved item form identity.
    pub object: FormId,
    /// Quantity in this distinct stack.
    pub count_delta: i32,
    /// CommonLib extra-data kind ids represented by this stack.
    pub extra_types: Vec<u8>,
    /// Whether the stack is worn or equipped.
    pub worn: bool,
}

/// A read-only inventory view for one placed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryChanges {
    /// Owning reference form identity.
    pub owner: FormId,
    /// Distinct stacks in deterministic item and stack order.
    pub entries: Vec<InventoryEntryData>,
    /// Authoritative totals after applying the runtime's inventory changes.
    pub totals: Vec<(FormId, i32)>,
}

impl InventoryChanges {
    /// Return the total quantity for an item form.
    #[must_use]
    pub fn get_item_count(&self, object: FormId) -> i32 {
        self.totals
            .iter()
            .find_map(|(form, count)| (*form == object).then_some(*count))
            .unwrap_or(0)
    }

    /// Inventory changes must use the engine's validated transaction path.
    ///
    /// # Errors
    /// Always returns [`Unsupported::InventoryMutation`].
    pub fn add_entry_data(&mut self, _entry: InventoryEntryData) -> Result<(), Unsupported> {
        Err(Unsupported::InventoryMutation)
    }
}

/// Menu-neutral item rows projected from a live inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemList {
    /// Current inventory rows.
    pub items: Vec<InventoryEntryData>,
}

impl ItemList {
    /// Return the selected menu row when a host UI provides one.
    ///
    /// # Errors
    /// Returns [`Unsupported::ItemSelection`] because the runtime has no
    /// Scaleform selection state.
    pub fn get_selected_item(&self) -> Result<&InventoryEntryData, Unsupported> {
        Err(Unsupported::ItemSelection)
    }

    /// Scaleform item-list refresh is not available in the headless runtime.
    ///
    /// # Errors
    /// Always returns [`Unsupported::ItemListUpdate`].
    pub fn update(&mut self) -> Result<(), Unsupported> {
        Err(Unsupported::ItemListUpdate)
    }
}

/// Whether a resolved reference is an actor or another placed object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A placed actor reference.
    Actor,
    /// A non-actor placed reference.
    Reference,
}

/// Result of looking up a source-level reference handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookupReferenceByHandle {
    /// Runtime form identity used as this shim's stable handle value.
    pub form_id: FormId,
    /// The concrete reference category.
    pub kind: ReferenceKind,
}

/// Convert an existing four-byte record signature to the shim form type.
#[must_use]
pub const fn form_type(signature: [u8; 4]) -> FormType {
    FormType(signature)
}

/// A form factory descriptor's signature.
#[must_use]
pub const fn factory_type(factory: &IFormFactory) -> FormType {
    factory.get_form_type()
}
