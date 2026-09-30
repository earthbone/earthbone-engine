// SPDX-License-Identifier: GPL-3.0-or-later

//! SkyUI MCM option data, rendered through Godot Controls.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveDialog {
    Slider(u32),
    Menu(u32),
    Color(u32),
}

/// The option row types supported by the MCM presenter.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionKind {
    Text {
        value: String,
    },
    Toggle {
        checked: bool,
    },
    Slider {
        value: f32,
        min_value: f32,
        max_value: f32,
        interval: f32,
    },
    Menu {
        value: String,
        options: Vec<String>,
        selected_index: usize,
    },
    Color {
        value: i32,
    },
    KeyMap {
        key_code: i32,
    },
}

/// One rendered MCM row with its SkyUI option ID and flags.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionRow {
    pub id: u32,
    pub label: String,
    pub kind: OptionKind,
    pub flags: u32,
}

/// Slider dialog parameters authored for an MCM option.
#[derive(Debug, Clone, PartialEq)]
pub struct SliderDialog {
    pub option_id: u32,
    pub start_value: f32,
    pub default_value: f32,
    pub min_value: f32,
    pub max_value: f32,
    pub interval: f32,
}

/// Menu dialog parameters authored for an MCM option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuDialog {
    pub option_id: u32,
    pub start_index: usize,
    pub default_index: usize,
    pub options: Vec<String>,
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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConfigPage {
    page: u32,
    next_slot: usize,
    rows: Vec<OptionRow>,
    active_dialog: Option<ActiveDialog>,
}

impl ConfigPage {
    /// Create an empty page whose IDs include its zero-based page number.
    pub fn new(page: u32) -> Self {
        Self {
            page,
            next_slot: 0,
            rows: Vec::new(),
            active_dialog: None,
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

    /// Add a slider option with its authored value range and interval.
    pub fn add_slider(
        &mut self,
        label: impl Into<String>,
        value: f32,
        min_value: f32,
        max_value: f32,
        interval: f32,
        flags: u32,
    ) -> Option<u32> {
        if !value.is_finite()
            || !min_value.is_finite()
            || !max_value.is_finite()
            || !interval.is_finite()
            || min_value > value
            || value > max_value
            || min_value >= max_value
            || interval <= 0.0
        {
            return None;
        }
        self.add(
            label.into(),
            OptionKind::Slider {
                value,
                min_value,
                max_value,
                interval,
            },
            flags,
        )
    }

    /// Add a menu option whose displayed value is one of its choices.
    pub fn add_menu(
        &mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        options: Vec<String>,
        flags: u32,
    ) -> Option<u32> {
        let value = value.into();
        let selected_index = options.iter().position(|option| option == &value)?;
        self.add(
            label.into(),
            OptionKind::Menu {
                value,
                options,
                selected_index,
            },
            flags,
        )
    }

    /// Add a color row and return its SkyUI option ID, or `None` at capacity.
    pub fn add_color(&mut self, label: impl Into<String>, value: i32, flags: u32) -> Option<u32> {
        self.add(label.into(), OptionKind::Color { value }, flags)
    }

    /// Add a key-map row and return its SkyUI option ID, or `None` at capacity.
    pub fn add_keymap(
        &mut self,
        label: impl Into<String>,
        key_code: i32,
        flags: u32,
    ) -> Option<u32> {
        self.add(label.into(), OptionKind::KeyMap { key_code }, flags)
    }

    fn add(&mut self, label: String, kind: OptionKind, flags: u32) -> Option<u32> {
        if self.next_slot >= PAGE_CAPACITY || self.page.checked_mul(PAGE_STRIDE).is_none() {
            return None;
        }
        let id = self.page * PAGE_STRIDE + u32::try_from(self.next_slot).ok()?;
        i32::try_from(id).ok()?;
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

    /// Dispatch the selection event for an enabled option row.
    /// Slider and menu rows dispatch their option-open event.
    /// Returns false for a missing, disabled, or hidden option.
    pub fn select(&mut self, game: &mut Game, script: GlobalFormId, id: u32) -> bool {
        let Some(row) = self.selectable_row(id) else {
            return false;
        };
        let kind = row.kind.clone();
        match kind {
            OptionKind::Slider { .. } => return self.open_slider(game, script, id).is_some(),
            OptionKind::Menu { .. } => return self.open_menu(game, script, id).is_some(),
            _ => {}
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        game.send_event(script, "OnOptionSelect", &[Value::Int(option_id)]);
        true
    }

    /// Dispatch the SkyUI slider-open event and return its authored parameters.
    pub fn open_slider(
        &mut self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
    ) -> Option<SliderDialog> {
        let row = self.selectable_row(id)?;
        let OptionKind::Slider {
            value,
            min_value,
            max_value,
            interval,
        } = &row.kind
        else {
            return None;
        };
        let option_id = i32::try_from(id).ok()?;
        if self.active_dialog.is_some() {
            return None;
        }
        let dialog = SliderDialog {
            option_id: id,
            start_value: *value,
            default_value: *value,
            min_value: *min_value,
            max_value: *max_value,
            interval: *interval,
        };
        self.active_dialog = Some(ActiveDialog::Slider(id));
        game.send_event(script, "OnOptionSliderOpen", &[Value::Int(option_id)]);
        Some(dialog)
    }

    /// Accept a slider value, clamp it to its authored range, and dispatch SkyUI's event.
    pub fn accept_slider(
        &mut self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
        selected_value: f32,
    ) -> bool {
        if self.active_dialog != Some(ActiveDialog::Slider(id)) {
            return false;
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.enabled() && row.visible())
        else {
            return false;
        };
        let OptionKind::Slider {
            value,
            min_value,
            max_value,
            interval,
        } = &mut row.kind
        else {
            return false;
        };
        if !selected_value.is_finite() {
            return false;
        }
        let clamped = selected_value.clamp(*min_value, *max_value);
        let steps = ((clamped - *min_value) / *interval).round();
        *value = (*min_value + steps * *interval).clamp(*min_value, *max_value);
        game.send_event(
            script,
            "OnOptionSliderAccept",
            &[Value::Int(option_id), Value::Float(*value)],
        );
        self.active_dialog = None;
        true
    }

    /// Dispatch the SkyUI menu-open event and return its authored choices.
    pub fn open_menu(
        &mut self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
    ) -> Option<MenuDialog> {
        let row = self.selectable_row(id)?;
        let OptionKind::Menu {
            options,
            selected_index,
            ..
        } = &row.kind
        else {
            return None;
        };
        let option_id = i32::try_from(id).ok()?;
        if self.active_dialog.is_some() {
            return None;
        }
        let dialog = MenuDialog {
            option_id: id,
            start_index: *selected_index,
            default_index: *selected_index,
            options: options.clone(),
        };
        self.active_dialog = Some(ActiveDialog::Menu(id));
        game.send_event(script, "OnOptionMenuOpen", &[Value::Int(option_id)]);
        Some(dialog)
    }

    /// Accept a menu choice by index and dispatch SkyUI's event.
    pub fn accept_menu(
        &mut self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
        selected_index: usize,
    ) -> bool {
        if self.active_dialog != Some(ActiveDialog::Menu(id)) {
            return false;
        }
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.id == id && row.enabled() && row.visible())
        else {
            return false;
        };
        let OptionKind::Menu {
            value,
            options,
            selected_index: current_index,
        } = &mut row.kind
        else {
            return false;
        };
        let Some(selected_value) = options.get(selected_index) else {
            return false;
        };
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        let Ok(selected_index_arg) = i32::try_from(selected_index) else {
            return false;
        };
        value.clone_from(selected_value);
        *current_index = selected_index;
        game.send_event(
            script,
            "OnOptionMenuAccept",
            &[Value::Int(option_id), Value::Int(selected_index_arg)],
        );
        self.active_dialog = None;
        true
    }

    /// Close an enabled slider or menu dialog without changing its value.
    #[must_use]
    pub fn cancel_dialog(&mut self, id: u32) -> bool {
        if !matches!(
            self.active_dialog,
            Some(ActiveDialog::Slider(open_id) | ActiveDialog::Menu(open_id)) if open_id == id
        ) {
            return false;
        }
        self.active_dialog = None;
        true
    }

    fn selectable_row(&self, id: u32) -> Option<&OptionRow> {
        let row = self.rows.iter().find(|row| row.id == id)?;
        (row.enabled() && row.visible()).then_some(row)
    }

    /// Send the SkyUI key-map callback with vanilla and mod conflict details.
    pub fn remap_key(
        &self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
        key_code: i32,
        conflict_control: &str,
        conflict_name: &str,
    ) -> bool {
        if !self.accepts_callback(id, |kind| matches!(kind, OptionKind::KeyMap { .. })) {
            return false;
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        game.send_event(
            script,
            "OnOptionKeyMapChange",
            &[
                Value::Int(option_id),
                Value::Int(key_code),
                Value::string(conflict_control),
                Value::string(conflict_name),
            ],
        );
        true
    }

    /// Open a color dialog and request its start and default colors from the config script.
    pub fn open_color_dialog(&mut self, game: &mut Game, script: GlobalFormId, id: u32) -> bool {
        if !self.accepts_callback(id, |kind| matches!(kind, OptionKind::Color { .. }))
            || self.active_dialog.is_some()
        {
            return false;
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        game.send_event(script, "OnOptionColorOpen", &[Value::Int(option_id)]);
        self.active_dialog = Some(ActiveDialog::Color(id));
        true
    }

    /// Accept a color only for the open option, then call the SkyUI accept callback.
    pub fn accept_color(
        &mut self,
        game: &mut Game,
        script: GlobalFormId,
        id: u32,
        color: i32,
    ) -> bool {
        if self.active_dialog != Some(ActiveDialog::Color(id))
            || !self.accepts_callback(id, |kind| matches!(kind, OptionKind::Color { .. }))
        {
            return false;
        }
        let Ok(option_id) = i32::try_from(id) else {
            return false;
        };
        game.send_event(
            script,
            "OnOptionColorAccept",
            &[Value::Int(option_id), Value::Int(color)],
        );
        self.active_dialog = None;
        true
    }

    /// Cancel the open color dialog without changing the option value or sending an accept event.
    pub fn cancel_color_dialog(&mut self, id: u32) -> bool {
        if self.active_dialog != Some(ActiveDialog::Color(id)) {
            return false;
        }
        self.active_dialog = None;
        true
    }

    /// Apply a color value supplied by the config script's accept callback.
    pub fn set_color_value(&mut self, id: u32, value: i32) -> bool {
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        let OptionKind::Color { value: current } = &mut row.kind else {
            return false;
        };
        *current = value;
        true
    }

    /// Apply a key code supplied by the config script's key-map callback.
    pub fn set_keymap_value(&mut self, id: u32, key_code: i32) -> bool {
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return false;
        };
        let OptionKind::KeyMap { key_code: current } = &mut row.kind else {
            return false;
        };
        *current = key_code;
        true
    }

    fn accepts_callback(&self, id: u32, accepts_kind: impl FnOnce(&OptionKind) -> bool) -> bool {
        self.selectable_row(id)
            .is_some_and(|row| accepts_kind(&row.kind))
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
            OptionKind::Text { value } | OptionKind::Menu { value, .. } => {
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
            OptionKind::Slider { value, .. } => {
                let mut button = Button::new_alloc();
                button.set_text(&value.to_string());
                button.set_disabled(!row.enabled());
                button.connect(
                    "pressed",
                    &callback.clone().bindv(&varray![i64::from(row.id)]),
                );
                line.add_child(&button);
            }
            OptionKind::Color { value } => {
                let mut button = Button::new_alloc();
                button.set_text(&format!("#{:08X}", *value as u32));
                button.set_disabled(!row.enabled());
                button.connect(
                    "pressed",
                    &callback.clone().bindv(&varray![i64::from(row.id)]),
                );
                line.add_child(&button);
            }
            OptionKind::KeyMap { key_code } => {
                let mut button = Button::new_alloc();
                button.set_text(&key_code.to_string());
                button.set_disabled(!row.enabled());
                button.connect(
                    "pressed",
                    &callback.clone().bindv(&varray![i64::from(row.id)]),
                );
                line.add_child(&button);
            }
        }
        content.add_child(&line);
    }
}
