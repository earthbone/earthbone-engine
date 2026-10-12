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

#[derive(Default)]
pub(crate) struct Bridge {
    subscriptions: BTreeMap<(ObjectId, String), String>,
    menus: std::collections::BTreeSet<(ObjectId, String)>,
    events: VecDeque<(ObjectId, String, String, f32)>,
    ui: BTreeMap<(String, String), UiCall>,
    menu_open: bool,
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
                    state.borrow_mut().ui.insert(
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
