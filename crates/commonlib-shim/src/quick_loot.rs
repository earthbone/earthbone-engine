// SPDX-License-Identifier: MIT

//! Source-level QuickLoot IE 2.0/2.1 API, derived only from the separately
//! MIT-licensed include/QuickLootAPI.h. Handles replace executable pointers,
//! and native item data replaces GFxValue. No GPL implementation lives here.

use std::collections::{BTreeMap, BTreeSet};

/// Runtime form identity, resolved by the engine adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(pub u64);

/// The upstream action ordinals, retained for source-level adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QuickLootAction {
    None,
    Disable,
    Enable,
    Use,
    Take,
    TakeAll,
    Transfer,
    ScrollUp,
    ScrollDown,
    PrevPage,
    NextPage,
}

/// Supported upstream interface generations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ApiVersion {
    V20,
    V21,
}

/// Mutable cancellation field on opening, taking and input events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum HandleResult {
    #[default]
    Continue,
    Stop,
}

/// An inventory entry view. It never owns the engine's inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStack {
    pub item: Handle,
    pub count: i32,
    /// Extra-list index, or None for the undescribed remainder.
    pub extra_index: Option<usize>,
    pub drop_ref: Option<Handle>,
}

/// Native item presentation values replacing Scaleform's GFxValue.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemValue {
    Text(String),
    Number(f64),
    Bool(bool),
}

/// Upstream button definition, including 2.1's stealing and action fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonDefinition {
    pub label: String,
    pub button_art_index: u16,
    pub stealing: bool,
    pub action: QuickLootAction,
}

/// All event registration points from the MIT API header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    TakingItem,
    TakeItem,
    SelectItem,
    OpeningLootMenu,
    OpenLootMenu,
    CloseLootMenu,
    InvalidateLootMenu,
    ModifyInventory,
    PopulateInfoBar,
    PopulateButtonBar,
    ModifyButtonBar,
    ModifyItemData,
    InputAction,
}

/// Native event envelope. Empty containers have no selected stack. Modification
/// events mutate the corresponding owned presentation vectors and data map.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub actor: Option<Handle>,
    pub container: Handle,
    pub stack: Option<ItemStack>,
    pub result: HandleResult,
    pub action: QuickLootAction,
    pub inventory: Vec<ItemStack>,
    pub info: Vec<String>,
    pub buttons: Vec<ButtonDefinition>,
    pub data: BTreeMap<String, ItemValue>,
}

impl Event {
    /// Build an event with Continue cancellation and empty presentation values.
    pub fn new(kind: EventKind, container: Handle) -> Self {
        Self {
            kind,
            container,
            actor: None,
            stack: None,
            result: HandleResult::Continue,
            action: QuickLootAction::None,
            inventory: Vec::new(),
            info: Vec::new(),
            buttons: Vec::new(),
            data: BTreeMap::new(),
        }
    }
}

/// Callback registration, with mutable events as in the upstream header.
pub type EventHandler = Box<dyn FnMut(&mut Event)>;

/// Commands queued by source-level mod callers for the host to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Close,
    Refresh,
    Action(QuickLootAction),
}

/// Initialization and version errors, never executable hook failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Uninitialized,
    EmptyPlugin,
    Version,
}

/// The MIT registration and request surface. The host owns dispatch and uses
/// the same instance that mods initialize. Menu and inventory logic are external.
#[derive(Default)]
pub struct QuickLootApi {
    plugins: BTreeMap<String, ApiVersion>,
    disabled: BTreeSet<String>,
    forced: Option<(String, Handle)>,
    handlers: Vec<(String, EventKind, EventHandler)>,
    requests: Vec<Request>,
}

impl QuickLootApi {
    /// Register a plugin before any other API call.
    pub fn init(&mut self, plugin: &str, version: ApiVersion) -> Result<(), Error> {
        if plugin.is_empty() {
            return Err(Error::EmptyPlugin);
        }
        self.plugins.insert(plugin.to_owned(), version);
        Ok(())
    }

    /// Check that a plugin initialized the requested minimum interface.
    pub fn is_ready(&self, plugin: &str, version: ApiVersion) -> bool {
        self.plugins
            .get(plugin)
            .is_some_and(|registered| version == ApiVersion::V20 || *registered == ApiVersion::V21)
    }

    fn check(&self, plugin: &str) -> Result<(), Error> {
        self.plugins
            .contains_key(plugin)
            .then_some(())
            .ok_or(Error::Uninitialized)
    }

    /// One mod's enable call clears only that mod's disable request.
    pub fn disable_loot_menu(&mut self, plugin: &str) -> Result<(), Error> {
        self.check(plugin)?;
        self.disabled.insert(plugin.to_owned());
        Ok(())
    }

    /// Remove the caller's disable request.
    pub fn enable_loot_menu(&mut self, plugin: &str) -> Result<(), Error> {
        self.check(plugin)?;
        self.disabled.remove(plugin);
        Ok(())
    }

    /// Whether any initialized mod has disabled normal crosshair presentation.
    pub fn enabled(&self) -> bool {
        self.disabled.is_empty()
    }

    /// Override the crosshair target until the caller clears its override.
    pub fn force_current_container(
        &mut self,
        plugin: &str,
        container: Handle,
    ) -> Result<(), Error> {
        self.check(plugin)?;
        self.forced = Some((plugin.to_owned(), container));
        Ok(())
    }

    /// Clear the caller's forced target.
    pub fn clear_forced_container(&mut self, plugin: &str) -> Result<(), Error> {
        self.check(plugin)?;
        if self
            .forced
            .as_ref()
            .is_some_and(|(owner, _)| owner == plugin)
        {
            self.forced = None;
        }
        Ok(())
    }

    /// Read the active forced target.
    pub fn forced_container(&self) -> Option<Handle> {
        self.forced.as_ref().map(|(_, handle)| *handle)
    }

    /// Register any API event. The three 2.1 events require that interface.
    pub fn register_handler(
        &mut self,
        plugin: &str,
        kind: EventKind,
        handler: EventHandler,
    ) -> Result<(), Error> {
        self.check(plugin)?;
        if matches!(
            kind,
            EventKind::ModifyButtonBar | EventKind::ModifyItemData | EventKind::InputAction
        ) && !self.is_ready(plugin, ApiVersion::V21)
        {
            return Err(Error::Version);
        }
        self.handlers.push((plugin.to_owned(), kind, handler));
        Ok(())
    }

    /// Deliver mutable events in registration order, retaining cancellation.
    pub fn dispatch(&mut self, event: &mut Event) {
        for (_, kind, handler) in &mut self.handlers {
            if *kind == event.kind {
                handler(event);
            }
            if event.result == HandleResult::Stop {
                break;
            }
        }
    }

    /// Queue Close, Refresh or a 2.1 input action for the next host update.
    pub fn request(&mut self, plugin: &str, request: Request) -> Result<(), Error> {
        self.check(plugin)?;
        if matches!(request, Request::Action(_)) && !self.is_ready(plugin, ApiVersion::V21) {
            return Err(Error::Version);
        }
        self.requests.push(request);
        Ok(())
    }

    /// Drain queued calls once, preserving call order.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }
}
