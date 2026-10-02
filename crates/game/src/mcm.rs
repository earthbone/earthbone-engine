// SPDX-License-Identifier: GPL-3.0-or-later

//! Native presentation bridge for SkyUI's `SKI_ConfigBase` contract.
//! Lifecycle order and the 128 menu limit follow the local SkyUI
//! `SKI_ConfigBase.psc` and `SKI_ConfigManager.psc` sources.

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::Value;

use crate::Game;

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
        let mut configs = Vec::new();
        for id in ids {
            if configs.len() == 128 {
                break;
            }
            if !self.world().quest(id).is_some_and(|quest| quest.running) {
                continue;
            }
            let object = self.object_of_form(id);
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
            if !self.mcm_registered.contains(&id) {
                self.mcm_registered.push(id);
                self.vm.send_event(object, "OnConfigRegister", &[]);
                self.run_until_idle(64);
            }
            configs.push(Config {
                form: id,
                name: name.to_string(),
                pages,
            });
        }
        configs
    }

    /// Open a registered config. SkyUI's `OpenConfig` resets the empty page
    /// before it sends `OnConfigOpen`.
    pub fn mcm_open(&mut self, form: GlobalFormId) -> bool {
        if !self.mcm_configs().iter().any(|config| config.form == form) {
            return false;
        }
        self.mcm_close();
        let object = self.object_of_form(form);
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
        if let Some(form) = self.mcm_active.take() {
            let object = self.object_of_form(form);
            if self.vm.call(object, "CloseConfig", &[]).is_ok() {
                self.run_until_idle(64);
            }
        }
    }
}
