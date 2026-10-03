// SPDX-License-Identifier: MIT

//! CommonLib world, inventory and reference contracts backed by the game runtime.
//!
//! Inventory and reference names follow local CommonLibSSE-NG headers. The
//! world provider uses stable load-order identity without importing engine
//! types. These are source-level views, not ABI-compatible Skyrim objects.
//! The settings and world-singleton views follow local CommonLibSSE-NG declarations.

use crate::form::{FormId, FormType};

/// Stable form identity at the shim boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormKey {
    pub file: u16,
    pub object: u32,
}

/// Live world reads used by the CommonLib-shaped world types.
pub trait WorldState {
    fn player(&mut self) -> Option<FormKey>;
    fn loaded_actors(&self) -> Vec<FormKey>;
    fn reference_position(&mut self, form: FormKey) -> Option<[f32; 3]>;
    fn actor_value(&mut self, form: FormKey, name: &str) -> Option<f32>;
    fn actor_is_dead(&mut self, form: FormKey) -> Option<bool>;
    /// Resolve a stable world form key to the runtime handle value.
    fn runtime_form_id(&mut self, form: FormKey) -> Option<u32>;
}

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
    /// Projectile creation, simulation and manager access are not implemented.
    ProjectileRuntime,
    /// Binary call-site and vtable patching cannot target this runtime.
    BinaryPatch,
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
            Self::ProjectileRuntime => formatter.write_str("projectile runtime is unsupported"),
            Self::BinaryPatch => formatter.write_str("binary patch hooks are unsupported"),
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
/// CommonLib's typed object-reference handle representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ObjectRefHandle(pub u32);

/// CommonLib's typed projectile handle representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ProjectileHandle(pub u32);

/// Read-only subset of the player singleton supported by the engine.
pub struct PlayerCharacter<'a, W: WorldState> {
    world: &'a mut W,
}

impl<'a, W: WorldState> PlayerCharacter<'a, W> {
    pub fn new(world: &'a mut W) -> Option<Self> {
        world.player()?;
        Some(Self { world })
    }

    pub fn form_key(&mut self) -> Option<FormKey> {
        self.world.player()
    }

    /// Return the player's reference handle in the runtime's resolved FormID space.
    pub fn handle(&mut self) -> Option<ObjectRefHandle> {
        let player = self.world.player()?;
        self.world.runtime_form_id(player).map(ObjectRefHandle)
    }

    pub fn actor_value(&mut self, name: &str) -> Option<f32> {
        let player = self.world.player()?;
        self.world.actor_value(player, name)
    }

    pub fn position(&mut self) -> Option<[f32; 3]> {
        let player = self.world.player()?;
        self.world.reference_position(player)
    }
}

/// The actor collection currently driven by the engine's world model.
pub struct ProcessLists<'a, W: WorldState> {
    world: &'a W,
}

impl<'a, W: WorldState> ProcessLists<'a, W> {
    pub fn new(world: &'a W) -> Self {
        Self { world }
    }

    pub fn actor_forms(&self) -> Vec<FormKey> {
        self.world.loaded_actors()
    }
}

/// A projectile runtime object is named for source compatibility, while its
/// runtime operations fail explicitly until projectile simulation is added.
pub struct Projectile {
    pub handle: ProjectileHandle,
}

impl Projectile {
    pub fn runtime(&self) -> Result<(), Unsupported> {
        Err(Unsupported::ProjectileRuntime)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Integer(i32),
    Float(f32),
    String(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub name: String,
    pub value: SettingValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SitSleepState {
    Normal = 0,
    WantToSit = 1,
    WaitingForSitAnimation = 2,
    Sitting = 3,
    WantToStand = 4,
    WantToSleep = 5,
    WaitingForSleepAnimation = 6,
    Sleeping = 7,
    WantToWake = 8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    pub hour: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReanimateEffect {
    pub id: u64,
    pub spell: FormId,
    pub effect: FormId,
    pub target: FormId,
    pub magnitude: f32,
    pub elapsed: f64,
    pub duration: Option<f64>,
}

/// Read-only access to live runtime values. Unsupported engine hooks return a
/// reason instead of manufacturing an address or silently doing nothing.
pub trait RuntimeState {
    fn setting(&self, name: &str) -> Option<Setting>;
    fn sky(&self) -> Sky;
    fn reanimate_effect(&self, id: u64) -> Option<ReanimateEffect>;
    fn sit_sleep_state(&self, actor: FormId) -> Result<SitSleepState, UnsupportedWorldHook>;
    fn hook(&self, name: &str) -> UnsupportedWorldHook;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedWorldHook {
    pub name: &'static str,
    pub reason: &'static str,
}

/// C-compatible values used by a host to expose [`RuntimeState`] to C++.
///
/// The callback table owns no runtime state. Its context pointer is borrowed
/// for the duration of each call, and implementations must return false when
/// the requested value is unavailable.
#[repr(C)]
pub struct RuntimeStateCallbacks {
    /// Read a setting into the tagged output record.
    pub setting: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *const std::ffi::c_char,
        *mut RuntimeSettingRecord,
    ) -> bool,
    /// Read the current calendar hour.
    pub sky_hour: unsafe extern "C" fn(*mut std::ffi::c_void) -> f32,
    /// Read an active reanimation effect into the output record.
    pub reanimate_effect:
        unsafe extern "C" fn(*mut std::ffi::c_void, u64, *mut RuntimeReanimateRecord) -> bool,
    /// Return the unsupported sit or sleep state diagnostic.
    pub sit_sleep_state: unsafe extern "C" fn(*mut std::ffi::c_void, u32, *mut RuntimeErrorRecord),
    /// Return the unsupported hook diagnostic.
    pub hook: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *const std::ffi::c_char,
        *mut RuntimeErrorRecord,
    ),
}

/// Fixed-layout setting payload for [`RuntimeStateCallbacks`].
#[repr(C)]
pub struct RuntimeSettingRecord {
    /// 0 bool, 1 integer, 2 float, 3 string.
    pub kind: u32,
    /// Bool or integer value.
    pub integer: i64,
    /// Floating point value.
    pub float: f64,
    /// NUL-terminated UTF-8 string value.
    pub string_value: [std::ffi::c_char; 256],
}

/// Fixed-layout reanimation payload for [`RuntimeStateCallbacks`].
#[repr(C)]
pub struct RuntimeReanimateRecord {
    /// Runtime effect id.
    pub id: u64,
    /// Packed spell form id.
    pub spell: u32,
    /// Packed magic effect form id.
    pub effect: u32,
    /// Packed target form id.
    pub target: u32,
    /// Effect magnitude.
    pub magnitude: f32,
    /// Elapsed game seconds.
    pub elapsed: f64,
    /// Whether duration is present.
    pub has_duration: bool,
    /// Duration when present.
    pub duration: f64,
}

/// Fixed-layout diagnostic payload for [`RuntimeStateCallbacks`].
#[repr(C)]
pub struct RuntimeErrorRecord {
    /// NUL-terminated diagnostic name.
    pub name: [std::ffi::c_char; 64],
    /// NUL-terminated diagnostic reason.
    pub reason: [std::ffi::c_char; 256],
}
