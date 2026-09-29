// SPDX-License-Identifier: GPL-3.0-or-later

//! SkyUI MCM text and toggle option data, rendered through Godot Controls.
//! Option IDs and disabled flags follow the local SkyUI SKI_ConfigBase API.

use soe_esm::GlobalFormId;
use soe_game::Game;
use soe_papyrus::Value;

#[cfg(feature = "godot")]
use godot::classes::{Button, CheckButton, HBoxContainer, Label, VBoxContainer};
#[cfg(feature = "godot")]
use godot::obj::NewAlloc;
#[cfg(feature = "godot")]
use godot::prelude::{varray, Callable, Gd};

pub const OPTION_FLAG_DISABLED: u32 = 0x01;
const PAGE_STRIDE: u32 = 0x100;
const PAGE_CAPACITY: usize = 128;

/// The option row types supported by UI-11.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionKind {
    Text { value: String },
    Toggle { checked: bool },
}

/// One rendered MCM row with its SkyUI option ID and flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionRow {
    pub id: u32,
    pub label: String,
    pub kind: OptionKind,
    pub flags: u32,
}

impl OptionRow {
    /// Whether selection is allowed by the row's current flags.
    #[must_use]
    pub fn enabled(&self) -> bool {
        self.flags & OPTION_FLAG_DISABLED == 0
    }

    /// Whether the SkyUI hidden flag permits presentation.
    #[must_use]
    pub fn visible(&self) -> bool {
        self.flags & 0x02 == 0
    }
}

/// A single page of MCM options using SkyUI's 128 positions per page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConfigPage {
    page: u32,
    next_slot: usize,
    rows: Vec<OptionRow>,
}

impl ConfigPage {
    /// Create an empty page whose IDs include its zero-based page number.
    pub fn new(page: u32) -> Self {
        Self {
            page,
            next_slot: 0,
            rows: Vec::new(),
        }
    }

    /// Add a text/value row and return its SkyUI option ID, or `None` at capacity.
    pub fn add_text(
        &mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        flags: u32,
    ) -> Option<u32> {
        self.add(
            label.into(),
            OptionKind::Text {
                value: value.into(),
            },
            flags,
        )
    }

    /// Add a toggle row and return its SkyUI option ID, or `None` at capacity.
    pub fn add_toggle(
        &mut self,
        label: impl Into<String>,
        checked: bool,
        flags: u32,
    ) -> Option<u32> {
        self.add(label.into(), OptionKind::Toggle { checked }, flags)
    }

    fn add(&mut self, label: String, kind: OptionKind, flags: u32) -> Option<u32> {
        if self.next_slot >= PAGE_CAPACITY || self.page.checked_mul(PAGE_STRIDE).is_none() {
            return None;
        }
        let id = self.page * PAGE_STRIDE + u32::try_from(self.next_slot).ok()?;
        self.rows.push(OptionRow {
            id,
            label,
            kind,
            flags,
        });
        self.next_slot += 1;
        Some(id)
    }

    /// Change whether a row can be selected, preserving all other flags.
    pub fn set_enabled(&mut self, id: u32, enabled: bool) -> bool {
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        if enabled {
            row.flags &= !OPTION_FLAG_DISABLED;
        } else {
            row.flags |= OPTION_FLAG_DISABLED;
        }
        true
    }

    /// Rows in authored order for rendering.
    #[must_use]
    pub fn rows(&self) -> &[OptionRow] {
        &self.rows
    }

    /// Dispatch one enabled row through the standard SKI selection event.
    /// Returns false for a missing, disabled, or hidden option.
    pub fn select(&self, game: &mut Game, script: GlobalFormId, id: u32) -> bool {
        let Some(row) = self.rows.iter().find(|row| row.id == id) else {
            return false;
        };
        if !row.enabled() || row.flags & 0x02 != 0 {
            return false;
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        game.send_event(script, "OnOptionSelect", &[Value::Int(option_id)]);
        true
    }
}

/// Build Godot rows for a page and bind each selection to the supplied host callback.
#[cfg(feature = "godot")]
pub fn render_rows(content: &mut Gd<VBoxContainer>, page: &ConfigPage, callback: &Callable) {
    for row in page.rows() {
        if !row.visible() {
            continue;
        }
        let mut line = HBoxContainer::new_alloc();
        let mut label = Label::new_alloc();
        label.set_text(&row.label);
        line.add_child(&label);
        match &row.kind {
            OptionKind::Text { value } => {
                let mut button = Button::new_alloc();
                button.set_text(value);
                button.set_disabled(!row.enabled());
                button.connect(
                    "pressed",
                    &callback.clone().bindv(&varray![i64::from(row.id)]),
                );
                line.add_child(&button);
            }
            OptionKind::Toggle { checked } => {
                let mut toggle = CheckButton::new_alloc();
                toggle.set_pressed(*checked);
                toggle.set_disabled(!row.enabled());
                toggle.connect(
                    "pressed",
                    &callback.clone().bindv(&varray![i64::from(row.id)]),
                );
                line.add_child(&toggle);
            }
        }
        content.add_child(&line);
    }
}
