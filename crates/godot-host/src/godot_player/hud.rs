// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot controls for the player status meters and heading compass.

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{CanvasLayer, ColorRect, Control, Label, ProgressBar, VBoxContainer};
use godot::prelude::*;

use super::PlayerNode;

pub(super) struct HudView {
    layer: Gd<CanvasLayer>,
    root: Gd<Control>,
    health_label: Gd<Label>,
    health_bar: Gd<ProgressBar>,
    magicka_label: Gd<Label>,
    magicka_bar: Gd<ProgressBar>,
    stamina_label: Gd<Label>,
    stamina_bar: Gd<ProgressBar>,
    compass: Gd<Label>,
    activation: Gd<Label>,
    cart_mode: Gd<Label>,
    blood_overlay: Gd<ColorRect>,
    presentation: crate::hud::GameUiPresentation,
}

impl HudView {
    fn new() -> Self {
        let mut layer = CanvasLayer::new_alloc();
        layer.set_name("HudLayer");
        layer.set_layer(10);
        let mut root = Control::new_alloc();
        root.set_name("Hud");
        root.set_mouse_filter(MouseFilter::IGNORE);
        root.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);

        let mut stats = VBoxContainer::new_alloc();
        stats.set_name("StatusMeters");
        stats.set_position(Vector2::new(24.0, 24.0));
        let health = meter_row(&mut stats, "Health", "HealthMeter");
        let magicka = meter_row(&mut stats, "Magicka", "MagickaMeter");
        let stamina = meter_row(&mut stats, "Stamina", "StaminaMeter");

        let mut compass = Label::new_alloc();
        compass.set_name("Compass");
        compass.set_position(Vector2::new(520.0, 24.0));
        compass.set_text("W   NW   [N]   NE   E");

        let mut activation = Label::new_alloc();
        activation.set_name("ActivationPrompt");
        activation.set_anchors_and_offsets_preset(LayoutPreset::CENTER);
        activation.set_offset(Side::LEFT, -70.0);
        activation.set_offset(Side::RIGHT, 70.0);
        activation.set_offset(Side::TOP, 24.0);
        activation.set_offset(Side::BOTTOM, 64.0);
        activation.hide();

        let mut cart_mode = Label::new_alloc();
        cart_mode.set_name("CartMode");
        cart_mode.set_anchors_and_offsets_preset(LayoutPreset::CENTER_TOP);
        cart_mode.set_position(Vector2::new(-42.0, 24.0));
        cart_mode.set_text("CART");
        cart_mode.hide();

        let mut blood_overlay = ColorRect::new_alloc();
        blood_overlay.set_name("ScreenBlood");
        blood_overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        blood_overlay.set_mouse_filter(MouseFilter::IGNORE);
        blood_overlay.set_color(Color::from_rgba(0.65, 0.0, 0.0, 0.0));
        blood_overlay.hide();

        root.add_child(&stats);
        root.add_child(&compass);
        root.add_child(&activation);
        root.add_child(&cart_mode);
        root.add_child(&blood_overlay);
        layer.add_child(&root);

        Self {
            layer,
            root,
            health_label: health.0,
            health_bar: health.1,
            magicka_label: magicka.0,
            magicka_bar: magicka.1,
            stamina_label: stamina.0,
            stamina_bar: stamina.1,
            compass,
            activation,
            cart_mode,
            blood_overlay,
            presentation: crate::hud::GameUiPresentation::default(),
        }
    }

    pub(super) fn refresh(&mut self, game: &mut soe_game::Game, yaw: f32) {
        let player = game.player();
        let snapshot = crate::hud::Snapshot::read(game, player, yaw);
        self.root.set_visible(true);
        set_meter(
            &mut self.health_label,
            &mut self.health_bar,
            "Health",
            snapshot.health,
        );
        set_meter(
            &mut self.magicka_label,
            &mut self.magicka_bar,
            "Magicka",
            snapshot.magicka,
        );
        set_meter(
            &mut self.stamina_label,
            &mut self.stamina_bar,
            "Stamina",
            snapshot.stamina,
        );
        self.compass.set_text(&snapshot.compass);
        let focused = game.world().crosshair_reference();
        let prompt = focused
            .filter(|speaker| game.player_dialogue_available(*speaker))
            .and_then(|speaker| game.world().name_of(speaker));
        self.activation.set_text(
            &prompt
                .as_ref()
                .map_or_else(String::new, |name| format!("Talk\n{name}")),
        );
        self.activation.set_visible(prompt.is_some());
    }

    pub(super) fn hide(&mut self) {
        self.root.hide();
    }

    pub(super) fn set_cart_mode(&mut self, enabled: bool) {
        self.presentation.set_cart_mode(enabled);
        self.cart_mode.set_visible(self.presentation.cart_mode);
    }

    pub(super) fn trigger_screen_blood(&mut self, value: i32) {
        self.presentation.trigger_screen_blood(value);
        self.blood_overlay.set_color(Color::from_rgba(
            0.65,
            0.0,
            0.0,
            self.presentation.blood_alpha,
        ));
        self.blood_overlay
            .set_visible(self.presentation.blood_alpha > 0.0);
    }

    pub(super) fn tick_presentation(&mut self, delta: f64) {
        self.presentation.tick(delta);
        self.blood_overlay.set_color(Color::from_rgba(
            0.65,
            0.0,
            0.0,
            self.presentation.blood_alpha,
        ));
        self.blood_overlay
            .set_visible(self.presentation.blood_alpha > 0.0);
    }
}

fn meter_row(
    parent: &mut Gd<VBoxContainer>,
    title: &str,
    bar_name: &str,
) -> (Gd<Label>, Gd<ProgressBar>) {
    let mut label = Label::new_alloc();
    label.set_text(title);
    parent.add_child(&label);
    let mut bar = ProgressBar::new_alloc();
    bar.set_name(bar_name);
    bar.set_show_percentage(false);
    bar.set_custom_minimum_size(Vector2::new(260.0, 18.0));
    parent.add_child(&bar);
    (label, bar)
}

fn set_meter(
    label: &mut Gd<Label>,
    bar: &mut Gd<ProgressBar>,
    name: &str,
    meter: crate::hud::Meter,
) {
    label.set_text(&format!(
        "{name}  {:.0} / {:.0}",
        meter.current, meter.maximum
    ));
    bar.set_value(f64::from(meter.fraction * 100.0));
}

impl PlayerNode {
    pub(super) fn create_hud(&mut self) {
        let view = HudView::new();
        self.base_mut().add_child(&view.layer);
        self.hud_view = Some(view);
    }
}
