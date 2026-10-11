// SPDX-License-Identifier: GPL-3.0-or-later

//! Native presentation bridge for SkyUI's `SKI_ConfigBase` contract.
//! Lifecycle order and the 128 menu limit follow the local SkyUI
//! `SKI_ConfigBase.psc` and `SKI_ConfigManager.psc` sources.
//! Native signatures and mod-event delivery follow local SKSE64's
//! scripts/modified/UI.psc and skse64/PapyrusForm.cpp.

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::{NativeReturn, Vm};
use soe_papyrus::{ObjectId, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use crate::Game;

/// One SKSE UI invocation for a native presenter. Array arguments are copied
/// at invocation time because SkyUI reuses its Papyrus buffers.
#[derive(Debug, Clone, PartialEq)]
pub struct UiCall {
    /// Bethesda menu name.
    pub menu: String,
    /// Scaleform function path identifying the presentation operation.
    pub target: String,
    /// Scalar arguments, or the elements of an array invocation.
    pub arguments: Vec<Value>,
}

/// One option transmitted by SkyUI's option buffers to its presenter.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionData {
    /// Original buffer position, including empty and hidden slots.
    pub index: u32,
    /// Low byte of SkyUI's packed option type and flags.
    pub option_type: u8,
    /// Authored option label.
    pub text: String,
    /// Authored string value.
    pub string_value: String,
    /// Authored numeric value, including toggle state.
    pub number_value: f32,
    /// SkyUI option flags.
    pub flags: u32,
}

#[derive(Default)]
struct Presentation {
    flags: Vec<Value>,
    text: Vec<Value>,
    strings: Vec<Value>,
    numbers: Vec<Value>,
    options: Vec<OptionData>,
    cursor: Option<usize>,
}

impl Presentation {
    fn apply(&mut self, call: &UiCall) {
        let Some(target) = call
            .target
            .strip_prefix("_root.ConfigPanelFader.configPanel.")
        else {
            return;
        };
        if call.menu != "Journal Menu" {
            return;
        }
        let buffer = match target {
            "setOptionFlagsBuffer" => Some(&mut self.flags),
            "setOptionTextBuffer" => Some(&mut self.text),
            "setOptionStrValueBuffer" => Some(&mut self.strings),
            "setOptionNumValueBuffer" => Some(&mut self.numbers),
            _ => None,
        };
        if let Some(buffer) = buffer {
            *buffer = call.arguments.iter().take(128).cloned().collect();
            return;
        }
        match (target, call.arguments.as_slice()) {
            ("flushOptionBuffers", [Value::Int(count)]) if (0..=128).contains(count) => {
                let options: Option<Vec<_>> = (0..*count as usize)
                    .map(|index| {
                        let Value::Int(packed) = self.flags.get(index)? else {
                            return None;
                        };
                        let Value::Str(text) = self.text.get(index)? else {
                            return None;
                        };
                        let string_value = match self.strings.get(index)? {
                            Value::Str(value) => value.to_string(),
                            Value::None => String::new(),
                            _ => return None,
                        };
                        let number_value = match self.numbers.get(index)? {
                            Value::Float(value) => *value,
                            Value::Int(value) => *value as f32,
                            _ => return None,
                        };
                        if !number_value.is_finite() {
                            return None;
                        }
                        Some(OptionData {
                            index: index as u32,
                            option_type: (*packed & 0xff) as u8,
                            text: text.to_string(),
                            string_value,
                            number_value,
                            flags: ((*packed as u32) >> 8) & 0xff,
                        })
                    })
                    .collect();
                if let Some(options) = options {
                    self.options = options;
                    self.cursor = None;
                    self.flags.clear();
                    self.text.clear();
                    self.strings.clear();
                    self.numbers.clear();
                }
            }
            ("setOptionFlags", [Value::Int(index), Value::Int(flags)]) => {
                if let Some(option) = usize::try_from(*index)
                    .ok()
                    .and_then(|index| self.options.get_mut(index))
                {
                    option.flags = *flags as u32;
                }
            }
            ("optionCursorIndex", [Value::Int(index)]) => {
                self.cursor = usize::try_from(*index)
                    .ok()
                    .filter(|index| *index < self.options.len());
            }
            ("optionCursor.strValue", [Value::Str(value)]) => {
                if let Some(option) = self.cursor.and_then(|index| self.options.get_mut(index)) {
                    option.string_value = value.to_string();
                }
            }
            ("optionCursor.numValue", [value @ (Value::Float(_) | Value::Int(_))])
                if value.as_float().is_finite() =>
            {
                if let Some(option) = self.cursor.and_then(|index| self.options.get_mut(index)) {
                    option.number_value = value.as_float();
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
pub(crate) struct Bridge {
    subscriptions: BTreeMap<(ObjectId, String), String>,
    menus: std::collections::BTreeSet<(ObjectId, String)>,
    events: VecDeque<(ObjectId, String, String, f32)>,
    ui: BTreeMap<(String, String), UiCall>,
    menu_open: bool,
    presentation: Presentation,
}

impl Bridge {
    pub(crate) fn has_pending_events(&self) -> bool {
        !self.events.is_empty()
    }
}

pub(crate) fn register(vm: &mut Vm) -> Rc<RefCell<Bridge>> {
    let bridge = Rc::new(RefCell::new(Bridge::default()));
    for (name, add) in [("RegisterForMenu", true), ("UnregisterForMenu", false)] {
        let state = Rc::clone(&bridge);
        vm.natives.register_method(
            "Form",
            name,
            Box::new(move |call| {
                if let (Some(object), Value::Str(menu)) = (call.self_object, call.arg(0)) {
                    let key = (object, menu.to_string());
                    if add {
                        state.borrow_mut().menus.insert(key);
                    } else {
                        state.borrow_mut().menus.remove(&key);
                    }
                }
                NativeReturn::Value(Value::None)
            }),
        );
    }
    let state = Rc::clone(&bridge);
    vm.natives.register_method(
        "Form",
        "RegisterForModEvent",
        Box::new(move |call| {
            if let (Some(object), Value::Str(event), Value::Str(callback)) =
                (call.self_object, call.arg(0), call.arg(1))
            {
                state
                    .borrow_mut()
                    .subscriptions
                    .insert((object, event.to_string()), callback.to_string());
            }
            NativeReturn::Value(Value::None)
        }),
    );
    let state = Rc::clone(&bridge);
    vm.natives.register_method(
        "Form",
        "UnregisterForModEvent",
        Box::new(move |call| {
            if let (Some(object), Value::Str(event)) = (call.self_object, call.arg(0)) {
                state
                    .borrow_mut()
                    .subscriptions
                    .remove(&(object, event.to_string()));
            }
            NativeReturn::Value(Value::None)
        }),
    );
    let state = Rc::clone(&bridge);
    vm.natives.register_method(
        "Form",
        "UnregisterForAllModEvents",
        Box::new(move |call| {
            if let Some(object) = call.self_object {
                state
                    .borrow_mut()
                    .subscriptions
                    .retain(|(owner, _), _| *owner != object);
            }
            NativeReturn::Value(Value::None)
        }),
    );
    let state = Rc::clone(&bridge);
    vm.natives.register_method(
        "Form",
        "SendModEvent",
        Box::new(move |call| {
            if let (Some(object), Value::Str(event)) = (call.self_object, call.arg(0)) {
                state.borrow_mut().events.push_back((
                    object,
                    event.to_string(),
                    match call.arg(1) {
                        Value::Str(value) => value.to_string(),
                        _ => String::new(),
                    },
                    call.arg(2).as_float(),
                ));
            }
            NativeReturn::Value(Value::None)
        }),
    );
    for name in [
        "Invoke",
        "InvokeBool",
        "InvokeInt",
        "InvokeFloat",
        "InvokeString",
        "InvokeBoolA",
        "InvokeIntA",
        "InvokeFloatA",
        "InvokeStringA",
        "SetInt",
        "SetFloat",
        "SetString",
    ] {
        let state = Rc::clone(&bridge);
        vm.natives.register_global(
            "UI",
            name,
            Box::new(move |call| {
                if let (Value::Str(menu), Value::Str(target)) = (call.arg(0), call.arg(1)) {
                    let arguments = match call.arg(2) {
                        Value::Array(array) => array.items.borrow().clone(),
                        Value::None => Vec::new(),
                        value => vec![value],
                    };
                    let invocation = UiCall {
                        menu: menu.to_string(),
                        target: target.to_string(),
                        arguments,
                    };
                    let mut state = state.borrow_mut();
                    state.presentation.apply(&invocation);
                    state.ui.insert(
                        (invocation.menu.clone(), invocation.target.clone()),
                        invocation,
                    );
                }
                NativeReturn::Value(Value::None)
            }),
        );
    }
    bridge
}

/// A live config quest and its script-declared menu labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The quest carrying the `SKI_ConfigBase` script.
    pub form: GlobalFormId,
    /// The script's `ModName` property.
    pub name: String,
    /// The script's `Pages` property, in declared order.
    pub pages: Vec<String>,
}

impl Game {
    /// Current native presentation data transmitted by the active config.
    /// This view is transient and does not replace the config's VM state.
    pub fn mcm_options(&self) -> Vec<OptionData> {
        if self.mcm_active.is_none() {
            return Vec::new();
        }
        self.mcm_bridge.borrow().presentation.options.clone()
    }

    /// Active SkyUI page number, zero for the empty page and one for the
    /// first named page. Minimal scripts without this variable use zero.
    pub fn mcm_page_number(&mut self) -> Option<u32> {
        let object = self.object_of_form(self.mcm_active?);
        match self.vm.get_var(object, "SKI_ConfigBase", "_currentPageNum") {
            Some(Value::Int(page)) => u32::try_from(page).ok(),
            None => Some(0),
            _ => None,
        }
    }

    /// Select a text or toggle option through the original manager when
    /// present, otherwise deliver the minimal fixture's direct callback.
    /// Hidden, disabled, stale-page and missing options are rejected.
    pub fn mcm_select_option(&mut self, form: GlobalFormId, id: u32) -> bool {
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        if !self.world().order().contains(form) {
            return false;
        }
        let object = self.object_of_form(form);
        let original = self
            .vm
            .has_script_method(object, "SKI_ConfigBase", "OnConfigManagerReady");
        if original || !self.mcm_bridge.borrow().presentation.options.is_empty() {
            if self.mcm_active != Some(form) || self.mcm_page_number() != Some(id / 0x100) {
                return false;
            }
            let selectable = self
                .mcm_bridge
                .borrow()
                .presentation
                .options
                .iter()
                .any(|option| {
                    option.index == id % 0x100
                        && matches!(option.option_type, 2 | 3)
                        && option.flags.trailing_zeros() >= 2
                });
            if !selectable {
                return false;
            }
        }
        if !original {
            return !self
                .send_event(form, "OnOptionSelect", &[Value::Int(option_id)])
                .is_empty();
        }
        if self.mcm_active != Some(form) {
            return false;
        }
        self.mcm_mod_event(form, "SKICP_optionSelected", "", (id % 0x100) as f32);
        true
    }

    /// Mark the host's MCM manager ready, register started config quests once,
    /// and read their current page lists.
    /// The entries are projections of attached script properties, not copies
    /// of authoritative menu data.
    pub fn mcm_configs(&mut self) -> Vec<Config> {
        let ids: Vec<_> = self
            .world()
            .order()
            .winners_of(Tag::new(b"QUST"))
            .map(|(id, _)| id)
            .collect();
        let manager = ids.iter().find_map(|&id| {
            if !self.world().quest(id).is_some_and(|quest| quest.running) {
                return None;
            }
            let object = self.object_of_form(id);
            self.vm
                .has_script_method(object, "SKI_ConfigManager", "RegisterMod")
                .then_some(object)
        });
        if let Some(manager) = manager {
            for &id in &ids {
                if !self.world().quest(id).is_some_and(|quest| quest.running) {
                    continue;
                }
                let object = self.object_of_form(id);
                if self
                    .vm
                    .has_script_method(object, "SKI_ConfigBase", "OnConfigManagerReady")
                    && !self
                        .mcm_bridge
                        .borrow()
                        .subscriptions
                        .contains_key(&(object, "SKICP_configManagerReady".into()))
                {
                    self.vm.send_event(object, "OnGameReload", &[]);
                }
            }
            if !self
                .mcm_bridge
                .borrow()
                .menus
                .contains(&(manager, "Journal Menu".into()))
            {
                self.vm.send_event(manager, "OnGameReload", &[]);
            }
            self.run_until_idle(64);
            self.mcm_bridge.borrow_mut().events.push_back((
                manager,
                "SKICP_configManagerReady".into(),
                String::new(),
                0.0,
            ));
            self.run_until_idle(64);
        }
        let mut configs = Vec::new();
        for id in ids {
            if configs.len() == 128 {
                break;
            }
            if !self.world().quest(id).is_some_and(|quest| quest.running) {
                continue;
            }
            let object = self.object_of_form(id);
            if self
                .vm
                .has_script_method(object, "SKI_ConfigBase", "OnConfigManagerReady")
            {
                if !matches!(self.vm.get_var(object, "SKI_ConfigBase", "_configID"), Some(Value::Int(id)) if id >= 0)
                    || !matches!(self.vm.get_var(object, "SKI_ConfigBase", "_configManager"), Some(Value::Object(owner)) if Some(owner) == manager)
                {
                    continue;
                }
                if !self.mcm_registered.contains(&id) {
                    self.mcm_registered.push(id);
                }
            } else if !self.mcm_registered.contains(&id) {
                if !matches!(
                    self.vm.get_property(object, "SKI_ConfigBase", "ModName"),
                    Some(Value::Str(_))
                ) {
                    continue;
                }
                self.mcm_registered.push(id);
                self.vm.send_event(object, "OnConfigRegister", &[]);
                self.run_until_idle(64);
            }
            let Some(Value::Str(name)) = self.vm.get_property(object, "SKI_ConfigBase", "ModName")
            else {
                continue;
            };
            let pages = match self.vm.get_property(object, "SKI_ConfigBase", "Pages") {
                Some(Value::Array(pages)) => pages
                    .items
                    .borrow()
                    .iter()
                    .map(|value| match value {
                        Value::Str(page) => page.to_string(),
                        _ => String::new(),
                    })
                    .collect(),
                Some(Value::None) => Vec::new(),
                _ => continue,
            };
            configs.push(Config {
                form: id,
                name: name.to_string(),
                pages,
            });
        }
        configs.sort_by_key(|config| {
            let object = self.object_of_form(config.form);
            match self.vm.get_var(object, "SKI_ConfigBase", "_configID") {
                Some(Value::Int(id)) => id,
                _ => i32::MAX,
            }
        });
        configs
    }

    /// Drain the latest invocation for each UI target. These are transient
    /// presentation requests, not script or world state.
    pub fn take_mcm_ui_calls(&mut self) -> Vec<UiCall> {
        std::mem::take(&mut self.mcm_bridge.borrow_mut().ui)
            .into_values()
            .collect()
    }

    pub(crate) fn pump_mcm_events(&mut self) -> usize {
        let mut dispatched = 0;
        let mut bridge = self.mcm_bridge.borrow_mut();
        while let Some((sender, event, string, number)) = bridge.events.pop_front() {
            for ((object, registered), callback) in &bridge.subscriptions {
                if registered == &event {
                    dispatched += self
                        .vm
                        .send_event(
                            *object,
                            callback,
                            &[
                                Value::string(&event),
                                Value::string(&string),
                                Value::Float(number),
                                Value::Object(sender),
                            ],
                        )
                        .len();
                }
            }
        }
        dispatched
    }

    fn mcm_menu_event(&mut self, event: &str) {
        for (object, menu) in &self.mcm_bridge.borrow().menus {
            if menu == "Journal Menu" {
                self.vm.send_event(*object, event, &[Value::string(menu)]);
            }
        }
        self.run_until_idle(64);
    }

    fn mcm_mod_event(&mut self, form: GlobalFormId, name: &str, string: &str, number: f32) {
        let object = self.object_of_form(form);
        self.mcm_bridge
            .borrow_mut()
            .events
            .push_back((object, name.into(), string.into(), number));
        self.run_until_idle(64);
    }

    /// Open the manager's Journal Menu lifecycle before presenting its list.
    /// Registration callbacks run before the manager enters its BUSY state.
    pub fn mcm_begin(&mut self) {
        self.mcm_configs();
        if !self.mcm_bridge.borrow().menu_open {
            self.mcm_bridge.borrow_mut().menu_open = true;
            self.mcm_menu_event("OnMenuOpen");
        }
    }

    /// Open a registered config. SkyUI's `OpenConfig` resets the empty page
    /// before it sends `OnConfigOpen`.
    pub fn mcm_open(&mut self, form: GlobalFormId) -> bool {
        if !self.mcm_configs().iter().any(|config| config.form == form) {
            return false;
        }
        let object = self.object_of_form(form);
        self.mcm_bridge.borrow_mut().presentation = Presentation::default();
        if let Some(Value::Int(id)) = self.vm.get_var(object, "SKI_ConfigBase", "_configID") {
            self.mcm_begin();
            self.mcm_mod_event(form, "SKICP_modSelected", "", id as f32);
            self.mcm_active = Some(form);
            return true;
        }
        self.mcm_close();
        if self.vm.call(object, "OpenConfig", &[]).is_err() {
            return false;
        }
        self.run_until_idle(64);
        self.mcm_active = Some(form);
        true
    }

    /// Select a declared page by index and deliver its reset callback.
    pub fn mcm_select_page(&mut self, index: usize) -> bool {
        let Some(form) = self.mcm_active else {
            return false;
        };
        let Some(config) = self.mcm_configs().into_iter().find(|c| c.form == form) else {
            return false;
        };
        let Some(page) = config.pages.get(index) else {
            return false;
        };
        let Ok(index) = i32::try_from(index) else {
            return false;
        };
        let object = self.object_of_form(form);
        if self
            .vm
            .has_script_method(object, "SKI_ConfigBase", "OnConfigManagerReady")
        {
            self.mcm_mod_event(form, "SKICP_pageSelected", page, index as f32);
            return true;
        }
        if self
            .vm
            .call(object, "SetPage", &[Value::string(page), Value::Int(index)])
            .is_err()
        {
            return false;
        }
        self.run_until_idle(64);
        true
    }

    /// Close the active config, if any, and deliver `OnConfigClose`.
    pub fn mcm_close(&mut self) {
        self.mcm_bridge.borrow_mut().presentation = Presentation::default();
        let closed_menu = self.mcm_bridge.borrow().menu_open;
        if closed_menu {
            self.mcm_bridge.borrow_mut().menu_open = false;
            self.mcm_menu_event("OnMenuClose");
        }
        if let Some(form) = self.mcm_active.take() {
            let object = self.object_of_form(form);
            if self
                .vm
                .has_script_method(object, "SKI_ConfigBase", "OnConfigManagerReady")
            {
                if !closed_menu {
                    self.mcm_menu_event("OnMenuClose");
                }
                return;
            }
            if self.vm.call(object, "CloseConfig", &[]).is_ok() {
                self.run_until_idle(64);
            }
        }
    }

    /// Query one registered config script for its custom control label.
    ///
    /// The call uses normal Papyrus method lookup, so a config override of
    /// `SKI_ConfigBase.GetCustomControl` is selected. `None` means the form
    /// is not a registered config or the method did not return a string.
    pub fn mcm_custom_control(&mut self, form: GlobalFormId, key_code: i32) -> Option<String> {
        let config = self
            .mcm_configs()
            .into_iter()
            .find(|config| config.form == form)?;
        self.call_mcm_custom_control(config.form, key_code)
    }

    /// Query registered configs in registration order for their custom
    /// control labels. Entries with a missing or non-string result are omitted.
    pub fn mcm_custom_controls(&mut self, key_code: i32) -> Vec<(Config, String)> {
        let configs = self.mcm_configs();
        configs
            .into_iter()
            .filter_map(|config| {
                self.call_mcm_custom_control(config.form, key_code)
                    .map(|control| (config, control))
            })
            .collect()
    }

    fn call_mcm_custom_control(&mut self, form: GlobalFormId, key_code: i32) -> Option<String> {
        let object = self.object_of_form(form);
        let thread = self
            .vm
            .call(object, "GetCustomControl", &[Value::Int(key_code)])
            .ok()?;
        self.run_until_idle(64);
        match self.vm.thread_result(thread)? {
            Value::Str(control) => Some(control.to_string()),
            _ => None,
        }
    }
}
