// SPDX-License-Identifier: GPL-3.0-or-later

//! Native modal UI from Godot Control/Theme and gdext node ownership examples.
//! PlayerNode keeps ticking the shared Game while this fixed control tree is open.

use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::input::MouseMode;
use godot::classes::{
    Button, CanvasLayer, Control, Input, Label, PanelContainer, StyleBoxFlat, Theme, VBoxContainer,
};
use godot::prelude::*;

use super::PlayerNode;

pub(super) struct MenuView {
    panel: Gd<PanelContainer>,
    title: Gd<Label>,
    content: Gd<VBoxContainer>,
    close: Gd<Button>,
    previous_mouse: Option<MouseMode>,
    previous_focus: Option<Gd<Control>>,
}

impl PlayerNode {
    pub(super) fn create_menu(&mut self) {
        let mut theme = Theme::new_gd();
        theme.set_default_font_size(22);
        theme.set_color("font_color", "Label", Color::from_rgb(0.94, 0.94, 0.96));
        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(Color::from_rgba(0.06, 0.07, 0.09, 0.98));
        style.set_content_margin_all(32.0);
        theme.set_stylebox("panel", "PanelContainer", &style);

        let mut canvas = CanvasLayer::new_alloc();
        canvas.set_name("MenuLayer");
        canvas.set_layer(100);
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("MenuPanel");
        panel.set_theme(&theme);
        panel.hide();
        let mut layout = VBoxContainer::new_alloc();
        layout.set_name("Layout");
        let mut title = Label::new_alloc();
        title.set_name("Title");
        let mut content = VBoxContainer::new_alloc();
        content.set_name("Content");
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut close = Button::new_alloc();
        close.set_name("Close");
        close.set_text("Close");
        close.connect("pressed", &self.base().callable("close_menu"));
        layout.add_child(&title);
        layout.add_child(&content);
        layout.add_child(&close);
        panel.add_child(&layout);
        canvas.add_child(&panel);
        self.base_mut().add_child(&canvas);
        panel.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.menu_view = Some(MenuView {
            panel,
            title,
            content,
            close,
            previous_mouse: None,
            previous_focus: None,
        });
    }

    /// Shared form-backed game access for native menu presenters.
    pub fn game(&self) -> Option<&soe_game::Game> {
        self.game.as_ref()
    }

    /// Mutations from a menu use the same Game as movement, scripts and activation.
    pub fn game_mut(&mut self) -> Option<&mut soe_game::Game> {
        self.game.as_mut()
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Open the modal shell. The framework owns controls added to menu_content.
    #[func]
    pub fn open_menu(&mut self, title: GString) -> bool {
        if !self.base().is_inside_tree() || self.menu_view.is_none() {
            return false;
        }
        if !self.menu.open(&title.to_string()) {
            return false;
        }
        self.container_menu = None;
        let previous_focus = self
            .base()
            .get_viewport()
            .and_then(|v| v.gui_get_focus_owner());
        let focused = self.input_focused;
        let previous_mouse = self
            .restore_mouse_on_focus
            .take()
            .unwrap_or_else(|| Input::singleton().get_mouse_mode());
        self.input_focus_lost();
        if focused {
            self.input_focus_gained();
        }
        let presentation = self.menu_view.as_mut().map(|view| {
            view.previous_mouse = Some(previous_mouse);
            view.previous_focus = previous_focus;
            (view.title.clone(), view.panel.clone(), view.close.clone())
        });
        Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
        if let Some((mut label, mut panel, mut close)) = presentation {
            // CONTEXT: Focus and visibility callbacks may query the player.
            let _guard = self.base_mut();
            label.set_text(&title);
            panel.show();
            close.grab_focus();
        }
        self.base_mut()
            .emit_signal("menu_opened", &[Variant::from(title)]);
        true
    }

    /// Close once, discard queued gameplay presses, and restore focus and mouse mode.
    #[func]
    pub fn close_menu(&mut self) -> bool {
        if !self.menu.close() {
            return false;
        }
        let focused = self.input_focused;
        self.input_focus_lost();
        if focused {
            self.input_focus_gained();
        }
        let presentation = self.menu_view.as_mut().map(|view| {
            (
                view.panel.clone(),
                view.content.clone(),
                view.previous_mouse.take(),
                view.previous_focus.take(),
            )
        });
        if let Some((mut panel, mut content, mouse, focus)) = presentation {
            if let Some(mode) = mouse {
                if focused {
                    Input::singleton().set_mouse_mode(mode);
                } else {
                    self.restore_mouse_on_focus = Some(mode);
                }
            }
            // CONTEXT: Presenter teardown and focus callbacks may query the player.
            let _guard = self.base_mut();
            if panel.is_instance_valid() {
                panel.hide();
            }
            if content.is_instance_valid() {
                for mut child in content.get_children().iter_shared() {
                    content.remove_child(&child);
                    child.queue_free();
                }
            }
            if let Some(mut focus) = focus {
                if focus.is_instance_valid() && focus.is_inside_tree() && focus.is_visible_in_tree()
                {
                    focus.grab_focus();
                }
            }
        }
        self.base_mut().emit_signal("menu_closed", &[]);
        true
    }

    #[func]
    pub fn is_menu_open(&self) -> bool {
        self.menu.is_open()
    }

    /// A native Control host for the current menu's presenter. None while closed.
    #[func]
    pub fn menu_content(&self) -> Option<Gd<Control>> {
        self.menu
            .is_open()
            .then_some(self.menu_view.as_ref())
            .flatten()
            .map(|view| view.content.clone().upcast())
    }

    /// Read the shared world clock for UI displays and host integration checks.
    #[func]
    pub fn gameplay_days(&self) -> f64 {
        self.game
            .as_ref()
            .map_or(0.0, |game| game.world().clock.game_days)
    }
}
