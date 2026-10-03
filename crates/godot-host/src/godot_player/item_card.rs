// SPDX-License-Identifier: GPL-3.0-or-later

//! Native item card and isolated rotating model viewport. SkyUI ItemCard.as
//! supplies the displayed fields, and godotwind preview_3d.gd supplies the
//! SubViewport, lighting and bounds-framing pattern. Uses existing host
//! surface/material builders rather than another NIF renderer.

use godot::classes::control::SizeFlags;
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::{
    Button, Camera3D, Control, DirectionalLight3D, HBoxContainer, Label, MeshInstance3D, Node3D,
    ScrollContainer, SubViewport, SubViewportContainer, VBoxContainer,
};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;
use crate::item_card::ItemCard;

pub(super) struct ItemCardView {
    name: Gd<Label>,
    statistics: Gd<Label>,
    status: Gd<Label>,
    pivot: Gd<Node3D>,
    selected: Option<GlobalFormId>,
}

impl ItemCardView {
    pub(super) fn create(parent: &mut Gd<HBoxContainer>) -> Self {
        Self::create_sized(parent, 220.0)
    }

    pub(super) fn create_sized(parent: &mut Gd<HBoxContainer>, width: f32) -> Self {
        let mut card = VBoxContainer::new_alloc();
        card.set_name("ItemCard");
        card.set_h_size_flags(SizeFlags::EXPAND_FILL);
        card.set_custom_minimum_size(Vector2::new(width, 0.0));
        let mut name = Label::new_alloc();
        name.set_name("ItemName");
        name.set_text("Select an item");
        name.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        name.add_theme_font_size_override("font_size", if width < 220.0 { 13 } else { 18 });
        card.add_child(&name);
        let mut container = SubViewportContainer::new_alloc();
        container.set_name("ModelPreview");
        container.set_custom_minimum_size(Vector2::new(width, 72.0));
        container.set_v_size_flags(SizeFlags::EXPAND_FILL);
        container.set_stretch(true);
        let mut viewport = SubViewport::new_alloc();
        viewport.set_name("Viewport");
        viewport.set_size(Vector2i::new(240, 180));
        viewport.set_use_own_world_3d(true);
        viewport.set_transparent_background(true);
        viewport.set_update_mode(UpdateMode::ALWAYS);
        let mut camera = Camera3D::new_alloc();
        camera.set_name("PreviewCamera");
        camera.set_position(Vector3::new(0.0, 0.0, 3.5));
        camera.set_near(0.05);
        camera.set_far(10.0);
        camera.set_fov(45.0);
        viewport.add_child(&camera);
        camera.make_current();
        for (rotation, energy) in [
            (Vector3::new(-0.4, -0.5, 0.0), 1.8),
            (Vector3::new(0.3, 2.4, 0.0), 0.8),
        ] {
            let mut light = DirectionalLight3D::new_alloc();
            light.set_rotation(rotation);
            light.set_param(godot::classes::light_3d::Param::ENERGY, energy);
            viewport.add_child(&light);
        }
        let mut pivot = Node3D::new_alloc();
        pivot.set_name("ItemModel");
        viewport.add_child(&pivot);
        container.add_child(&viewport);
        card.add_child(&container);
        let mut statistics = Label::new_alloc();
        statistics.set_name("Statistics");
        statistics.add_theme_font_size_override("font_size", if width < 220.0 { 12 } else { 16 });
        card.add_child(&statistics);
        let mut status = Label::new_alloc();
        status.set_name("PreviewStatus");
        status.hide();
        status.add_theme_font_size_override("font_size", 14);
        card.add_child(&status);
        parent.add_child(&card);
        Self {
            name,
            statistics,
            status,
            pivot,
            selected: None,
        }
    }

    pub(super) fn tick(&mut self, delta: f64) {
        if self.pivot.is_instance_valid() && self.pivot.get_child_count() > 0 {
            self.pivot.rotate_y((delta * 0.4) as f32);
        }
    }
}

/// Select a row with keyboard focus or mouse hover without activating its action.
pub(super) fn connect_preview(base: &Gd<Node3D>, button: &mut Gd<Button>, item: GlobalFormId) {
    button.set_clip_text(true);
    let callback = base
        .callable("preview_inventory_item")
        .bindv(&varray![i64::from(item.file), i64::from(item.object)]);
    button.connect("focus_entered", &callback);
    button.connect("mouse_entered", &callback);
}

impl PlayerNode {
    pub(super) fn item_menu_rows(
        &mut self,
        content: &mut Gd<Control>,
        selected: Option<GlobalFormId>,
    ) -> Gd<VBoxContainer> {
        self.item_card_view = None;
        for mut child in content.get_children().iter_shared() {
            content.remove_child(&child);
            child.queue_free();
        }
        let mut split = HBoxContainer::new_alloc();
        split.set_name("ItemMenu");
        split.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_name("ItemList");
        scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        let mut rows = VBoxContainer::new_alloc();
        rows.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.add_child(&rows);
        split.add_child(&scroll);
        let view = ItemCardView::create(&mut split);
        content.add_child(&split);
        self.item_card_view = Some(view);
        if let Some(item) = selected {
            self.preview_inventory_item(i64::from(item.file), i64::from(item.object));
        }
        rows
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Show a selected record in the active menu's shared item card.
    #[func]
    pub fn preview_inventory_item(&mut self, file: i64, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        let item = GlobalFormId { file, object };
        if self.item_card_view.is_none() && self.quick_loot_card.is_none() {
            return false;
        }
        if self
            .active_item_card()
            .as_ref()
            .is_some_and(|view| view.selected == Some(item))
        {
            return true;
        }
        let Some(game) = &self.game else { return false };
        let card = ItemCard::read(&mut game.world_mut(), item);
        let name = crate::item_card::display_name(&game.world(), item);
        let surfaces = card
            .as_ref()
            .ok()
            .and_then(|card| card.model.as_ref())
            .and_then(|model| {
                self.data_source
                    .as_ref()
                    .map(|source| source.item_preview_surfaces(model))
            });
        let Some(view) = (if self.menu.is_open() { self.item_card_view.as_mut() } else { self.quick_loot_card.as_mut() }) else {
            return false;
        };
        view.selected = None;
        view.name.set_text(&name);
        view.statistics.set_text(
            &card
                .as_ref()
                .map_or_else(|_| "Item details unavailable".into(), ItemCard::statistics),
        );
        view.pivot.set_rotation(Vector3::new(0.7, 0.4, 0.0));
        for mut child in view.pivot.get_children().iter_shared() {
            view.pivot.remove_child(&child);
            child.queue_free();
        }
        let surfaces = match surfaces {
            Some(Ok(surfaces)) if !surfaces.is_empty() => surfaces,
            Some(Err(error)) => {
                godot_warn!("Item preview: {error}");
                view.status.set_text("Model unavailable");
                view.status.show();
                return false;
            }
            _ => {
                view.status.set_text("No inventory model");
                view.status.show();
                view.selected = card.as_ref().ok().map(|_| item);
                return card.is_ok();
            }
        };
        let mut min = Vector3::splat(f32::INFINITY);
        let mut max = Vector3::splat(f32::NEG_INFINITY);
        for (arrays, _) in &surfaces {
            for position in &arrays.positions {
                let point = Vector3::from_array(*position);
                min = Vector3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z));
                max = Vector3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z));
            }
        }
        let center = (min + max) * 0.5;
        let radius = ((max - min).length() * 0.5).max(0.001);
        for (mut arrays, textures) in surfaces {
            for position in &mut arrays.positions {
                *position = ((Vector3::from_array(*position) - center) / radius).to_array();
            }
            let mut mesh = crate::godot_layer::array_mesh_from(&arrays);
            let material = crate::godot_layer::standard_material_from(&arrays.material, &textures);
            mesh.surface_set_material(0, &material);
            let mut instance = MeshInstance3D::new_alloc();
            instance.set_mesh(&mesh);
            view.pivot.add_child(&instance);
        }
        view.status.set_text("");
        view.status.hide();
        view.selected = Some(item);
        true
    }
}
