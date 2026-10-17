// SPDX-License-Identifier: GPL-3.0-or-later

//! Native console overlay following UESP Skyrim Console and Godot Control input.
//! One fixed control tree belongs to PlayerNode, with no per-reference nodes.

use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::input::MouseMode;
use godot::classes::{
    CanvasLayer, Input, InputEvent, InputEventKey, InputEventMouseButton, Label, LineEdit,
    PanelContainer, PhysicsRayQueryParameters3D, PhysicsServer3D, RichTextLabel, VBoxContainer,
};
use godot::global::{Key, MouseButton};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::debug::{Console, HostRequest};
use soe_physics::PhysicsWorld;

use super::PlayerNode;
mod ini;

#[derive(Default)]
pub(super) struct Session {
    pub open: bool,
    commands: Console,
    view: Option<View>,
    history: Vec<String>,
    cursor: usize,
    draft: String,
    output: std::collections::VecDeque<String>,
    prior_mouse: Option<MouseMode>,
    prior_focus: Option<Gd<godot::classes::Control>>,
    menus_visible: bool,
    lod_visible: bool,
    pub free_camera: Option<FreeCamera>,
    pub sky_hidden: bool,
    pub imagespace_disabled: bool,
    pub show_first_person: bool,
    pub animator_camera: Option<(crate::camera::CameraMode, f32)>,
    wireframe: bool,
    free_speed: f32,
}

pub(super) struct FreeCamera {
    position: soe_physics::Vec3,
    prior_facing: (f32, f32),
    freeze: bool,
}

struct View {
    panel: Gd<PanelContainer>,
    selected: Gd<Label>,
    output: Gd<RichTextLabel>,
    input: Gd<LineEdit>,
}

fn selectable_reference(world: &soe_game::World, id: GlobalFormId) -> bool {
    match world.tag_of(id) {
        Some(tag) => tag == soe_esm::tag::REFR || tag == soe_esm::Tag(*b"ACHR"),
        None => world.reference_ref(id).is_some(),
    }
}

impl PlayerNode {
    pub(super) fn create_console(&mut self) {
        godot::classes::RenderingServer::singleton().set_debug_generate_wireframes(true);
        let mut layer = CanvasLayer::new_alloc();
        layer.set_name("ConsoleLayer");
        layer.set_layer(100);
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("Console");
        panel.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        panel.set_anchor(godot::builtin::Side::TOP, 0.55);
        let mut rows = VBoxContainer::new_alloc();
        rows.set_name("Content");
        let mut selected = Label::new_alloc();
        selected.set_name("Selected");
        selected.set_text("No reference selected");
        let mut output = RichTextLabel::new_alloc();
        output.set_name("Output");
        output.set_v_size_flags(SizeFlags::EXPAND_FILL);
        output.set_scroll_follow(true);
        output.set_use_bbcode(false);
        let mut input = LineEdit::new_alloc();
        input.set_name("Input");
        input.set_placeholder("Console command");
        input.connect("text_submitted", &self.base().callable("console_execute"));
        rows.add_child(&selected);
        rows.add_child(&output);
        rows.add_child(&input);
        panel.add_child(&rows);
        layer.add_child(&panel);
        self.base_mut().add_child(&layer);
        panel.hide();
        self.console.view = Some(View {
            panel,
            selected,
            output,
            input,
        });
        self.console.menus_visible = true;
        self.console.lod_visible = true;
        self.console.free_speed = 10.0;
    }

    pub(super) fn console_input(&mut self, event: &Gd<InputEvent>) -> bool {
        if let Ok(key) = event.clone().try_cast::<InputEventKey>() {
            let toggle = key.get_keycode() == Key::QUOTELEFT
                || key.get_physical_keycode() == Key::QUOTELEFT
                || matches!(key.get_unicode(), 96 | 126);
            if toggle {
                if key.is_pressed() && !key.is_echo() {
                    self.console_set_open(!self.console.open);
                }
                return true;
            }
            if self.console.open && key.is_pressed() {
                match key.get_keycode() {
                    Key::ESCAPE => {
                        self.console_set_open(false);
                        return true;
                    }
                    Key::UP => {
                        self.console_history(true);
                        return true;
                    }
                    Key::DOWN => {
                        self.console_history(false);
                        return true;
                    }
                    Key::PAGEUP | Key::PAGEDOWN => {
                        if let Some(view) = self.console.view.as_mut() {
                            let Some(mut bar) = view.output.get_v_scroll_bar() else {
                                return true;
                            };
                            let direction = if key.get_keycode() == Key::PAGEUP {
                                -1.0
                            } else {
                                1.0
                            };
                            let value = bar.get_value() + direction * bar.get_page();
                            bar.set_value(value);
                        }
                        return true;
                    }
                    _ => {}
                }
            }
        }
        if self.console.open {
            if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
                if button.is_pressed()
                    && button.get_button_index() == MouseButton::LEFT
                    && self.console.view.as_ref().is_some_and(|v| {
                        !v.panel
                            .get_global_rect()
                            .contains_point(button.get_position())
                    })
                {
                    let picked = self.console_pick(button.get_position());
                    self.console_select(picked);
                }
            }
            // CONTEXT: Do not mark text events handled, LineEdit receives them in GUI input.
            return false;
        }
        if self.console.animator_camera.is_some() {
            if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
                let zoom = match button.get_button_index() {
                    MouseButton::WHEEL_UP => -20.0,
                    MouseButton::WHEEL_DOWN => 20.0,
                    _ => 0.0,
                };
                if zoom != 0.0 {
                    if button.is_pressed() {
                        self.rig.settings.boom_length =
                            (self.rig.settings.boom_length + zoom).clamp(20.0, 2000.0);
                        self.update_camera();
                    }
                    return true;
                }
            }
        }
        false
    }

    fn console_history(&mut self, up: bool) {
        let Some(view) = self.console.view.as_mut() else {
            return;
        };
        if self.console.cursor == self.console.history.len() {
            self.console.draft = view.input.get_text().to_string();
        }
        self.console.cursor = if up {
            self.console.cursor.saturating_sub(1)
        } else {
            (self.console.cursor + 1).min(self.console.history.len())
        };
        let line = self
            .console
            .history
            .get(self.console.cursor)
            .unwrap_or(&self.console.draft);
        view.input.set_text(line);
        view.input.set_caret_column(line.chars().count() as i32);
    }

    fn console_print(&mut self, text: &str) {
        for line in text.lines() {
            self.console.output.push_back(line.to_owned());
        }
        while self.console.output.len() > 2000 {
            self.console.output.pop_front();
        }
        if let Some(view) = self.console.view.as_mut() {
            let text = self
                .console
                .output
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            view.output.set_text(&text);
        }
    }

    fn console_select(&mut self, picked: Option<GlobalFormId>) {
        if picked == self.console.commands.selected() {
            self.console.commands.deselect();
        } else if let Some(id) = picked {
            self.console.commands.select(id);
        } else {
            self.console.commands.deselect();
        }
        self.console_selection_label();
    }

    fn console_selection_label(&mut self) {
        let label =
            self.console
                .commands
                .selected()
                .and_then(|id| {
                    let game = self.game.as_mut()?;
                    let deleted = game.world().reference_ref(id).is_some_and(|reference| {
                        reference.console_delete_pending || reference.deleted
                    });
                    Some(format!(
                        "{} ({:08X}){}",
                        game.world().name_of(id).unwrap_or_default(),
                        game.world().runtime_id(id),
                        if deleted { " [D]" } else { "" },
                    ))
                })
                .unwrap_or_else(|| "No reference selected".into());
        if let Some(view) = self.console.view.as_mut() {
            view.selected.set_text(&label);
        }
    }

    fn console_pick(&mut self, point: Vector2) -> Option<GlobalFormId> {
        let camera = self.camera.as_ref()?;
        let from = camera.project_ray_origin(point);
        let to = from + camera.project_ray_normal(point) * 10000.0;
        let space = self.base().get_world_3d()?.get_space();
        let mut query = PhysicsRayQueryParameters3D::new_gd();
        query.set_from(from);
        query.set_to(to);
        query.set_hit_from_inside(true);
        let player = self.game.as_mut()?.player();
        let excluded: Array<Rid> = self
            .actor_bodies
            .owners()
            .iter()
            .filter_map(|(rid, id)| (*id == player).then_some(*rid))
            .collect();
        query.set_exclude(&excluded);
        let mut state = PhysicsServer3D::singleton().space_get_direct_state(space)?;
        let hit = state.intersect_ray(&query);
        let body = hit.get("rid").and_then(|value| value.try_to::<Rid>().ok());
        let physical = body.and_then(|body| {
            self.query_collision
                .reference_for_body(body)
                .or_else(|| self.game_streamer.as_ref()?.bind().reference_for_body(body))
                .or_else(|| self.actor_bodies.owners().get(&body).copied())
                .or_else(|| self.npc_actors.console_reference_for_body(body))
        });
        let mut nearest = hit
            .get("position")
            .and_then(|value| value.try_to::<Vector3>().ok())
            .map(|point| (point.distance_to(from) / from.distance_to(to), physical));
        let actor = self.npc_actors.console_geometry_pick(from, to);
        let reference = self
            .game_streamer
            .as_ref()
            .and_then(|streamer| streamer.bind().console_geometry_pick(from, to));
        let own_body = self
            .player_appearance
            .as_ref()
            .and_then(|visual| visual.bind().console_geometry_hit(from, to))
            .map(|fraction| (fraction, player));
        for (fraction, reference) in actor.into_iter().chain(reference).chain(own_body) {
            if nearest.is_none_or(|old| fraction <= old.0 + f32::EPSILON) {
                nearest = Some((fraction, Some(reference)));
            }
        }
        let picked = nearest?.1?;
        selectable_reference(&self.game.as_ref()?.world(), picked).then_some(picked)
    }

    pub(super) fn console_noclip_step(
        &mut self,
        delta: f64,
        input: &crate::input::InputState,
    ) -> bool {
        if self
            .game
            .as_ref()
            .is_none_or(|game| game.world().debug_collisions_enabled)
        {
            return false;
        }
        let direction =
            self.rig.look_direction() * input.move_y + self.rig.right_flat() * input.move_x;
        let position = self.feet() + direction * (400.0 * delta as f32);
        if let Some(world) = self.world.as_mut() {
            if let Some(character) = self.character.take() {
                world.remove(character);
            }
            self.character =
                Some(world.create_character(soe_physics::CharacterSettings::default(), position));
        }
        true
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Toggle console focus and pause world input. Previous mouse mode is restored on close.
    #[func]
    pub fn console_set_open(&mut self, open: bool) -> bool {
        if open == self.console.open {
            return open;
        }
        self.console.open = open;
        self.release_carry();
        self.clear_player_input();
        self.devices = crate::input::devices::Devices::default();
        self.held_actions.clear();
        self.action_latches.clear();
        self.pending_look = (0.0, 0.0);
        if let Some(worker) = self.simulation.as_ref() {
            if !worker.command(soe_game::simulation::Command::MenuMode(
                open || self.menu.is_open(),
            )) {
                self.console_print("error: simulation pause queue is full");
            }
        }
        if open {
            self.console.prior_focus = self
                .base()
                .get_viewport()
                .and_then(|v| v.gui_get_focus_owner());
        }
        if let Some(view) = self.console.view.as_mut() {
            view.panel.set_visible(open);
            if open {
                self.console.prior_mouse = Some(Input::singleton().get_mouse_mode());
                Input::singleton().set_mouse_mode(MouseMode::VISIBLE);
                view.input.grab_focus();
            } else {
                view.input.release_focus();
                if let Some(mode) = self.console.prior_mouse.take() {
                    Input::singleton().set_mouse_mode(mode);
                }
            }
        }
        if !open {
            if let Some(mut focus) = self.console.prior_focus.take() {
                if focus.is_instance_valid() && focus.is_inside_tree() && focus.is_visible_in_tree()
                {
                    focus.grab_focus();
                }
            }
        }
        open
    }

    /// Whether the console currently owns input, even when TM hides its controls.
    #[func]
    pub fn console_is_open(&self) -> bool {
        self.console.open
    }

    /// Current console presentation switches for host diagnostics.
    #[func]
    pub fn console_features(&self) -> VarDictionary {
        let mut result = VarDictionary::new();
        result.set("sky", !self.console.sky_hidden);
        result.set("imagespace", !self.console.imagespace_disabled);
        result.set("first_person", self.console.show_first_person);
        result.set("animcam", self.console.animator_camera.is_some());
        result.set("wireframe", self.console.wireframe);
        result
    }

    /// Execute through the same Console as scenario tests, retaining selection and history.
    #[func]
    pub fn console_execute(&mut self, line: GString) -> GString {
        let line = String::from(line);
        if line.trim().is_empty() {
            return "".into();
        }
        self.console_print(&format!("> {line}"));
        self.console.history.push(line.clone());
        if self.console.history.len() > 500 {
            self.console.history.remove(0);
        }
        self.console.cursor = self.console.history.len();
        self.console.draft.clear();
        if let Some(view) = self.console.view.as_mut() {
            view.input.clear();
        }
        let result = if let Some(game) = self.game.as_mut() {
            self.console.commands.run(game, &line)
        } else {
            Err("console commands require the shared gameplay runtime, threaded draft commands are not bridged".into())
        };
        let mut output =
            result.map_or_else(|error| format!("error: {error}"), |value| value.to_string());
        for request in self.console.commands.take_host_requests() {
            if let Err(error) = self.console_host_request(request) {
                output.push_str(&format!("\nerror: {error}"));
            }
        }
        if self.game.is_some() {
            self.process_console_quit();
            if !self.sync_game_pose() {
                output.push_str("\nerror: host could not install the new player pose");
            }
            self.sync_door_presentation();
        }
        self.console_selection_label();
        self.console_print(&output);
        GString::from(&output)
    }
}

impl PlayerNode {
    fn process_console_quit(&mut self) {
        let quit = self.game.as_ref().is_some_and(|game| {
            game.world()
                .debug_host_requests
                .iter()
                .any(|r| matches!(r, soe_game::world::DebugHostRequest::Quit))
        });
        if quit {
            self.base().get_tree().quit();
        }
    }

    fn console_host_request(&mut self, request: HostRequest) -> Result<(), String> {
        match request {
            HostRequest::FreeCamera(freeze) => {
                if let Some(camera) = self.console.free_camera.take() {
                    self.rig
                        .set_facing(camera.prior_facing.0, camera.prior_facing.1);
                    self.update_camera();
                } else {
                    let position = self
                        .camera
                        .as_ref()
                        .map(|camera| {
                            let position = camera.get_global_position();
                            let position =
                                crate::units::GodotPos::new(position.x, position.y, position.z)
                                    .to_skyrim();
                            soe_physics::Vec3::new(position.x, position.y, position.z)
                        })
                        .unwrap_or_else(|| self.feet());
                    self.console.free_camera = Some(FreeCamera {
                        position,
                        prior_facing: (self.rig.yaw(), self.rig.pitch()),
                        freeze,
                    });
                }
                self.clear_player_input();
            }
            HostRequest::FreeCameraSpeed(value) => self.console.free_speed = value,
            HostRequest::Fov(value) => {
                self.rig.settings.world_fov = value;
                self.rig.settings.first_person_fov = value;
                self.update_camera();
            }
            HostRequest::TimeMultiplier(value) => {
                godot::classes::Engine::singleton().set_time_scale(value);
            }
            HostRequest::Container(id) => {
                self.console_set_open(false);
                if !self.open_container_menu(id) {
                    return Err("could not open actor container".into());
                }
            }
            HostRequest::RaceMenu => {
                self.console_set_open(false);
                if !self.show_race_menu() {
                    return Err("could not open race menu".into());
                }
            }
            HostRequest::Save { name, load } => {
                let path = GString::from(&format!("user://saves/console/{name}"));
                let result = if load {
                    self.recover_game_generation(path, 3)
                } else {
                    self.save_game_generation(path, 3)
                };
                if !result.get("ok").is_some_and(|v| v.to::<bool>()) {
                    return Err(result.get("error").map_or_else(
                        || "save operation failed".into(),
                        |v| v.to::<GString>().to_string(),
                    ));
                }
            }
            HostRequest::Toggle(name) => self.console_toggle_presentation(&name)?,
            HostRequest::Purge => {
                self.interior_prefetch.invalidate();
                if let Some(source) = self.data_source.as_ref() {
                    source.clear_prepared_textures();
                }
            }
            HostRequest::Ini { reload } => self.console_ini(reload)?,
        }
        Ok(())
    }
}

impl PlayerNode {
    fn console_toggle_presentation(&mut self, name: &str) -> Result<(), String> {
        match name {
            "animcam" => {
                if let Some((mode, yaw)) = self.console.animator_camera.take() {
                    self.rig.mode = mode;
                    self.rig.set_facing(yaw, self.rig.pitch());
                } else {
                    self.console.animator_camera = Some((self.rig.mode, self.rig.yaw()));
                    self.rig.mode = crate::camera::CameraMode::Third;
                }
                self.sync_camera_controls();
                self.update_camera();
            }
            "s1st" => {
                self.console.show_first_person = !self.console.show_first_person;
                self.sync_camera_controls();
            }
            "ts" => self.console.sky_hidden = !self.console.sky_hidden,
            "teofis" => {
                self.console.imagespace_disabled = !self.console.imagespace_disabled;
                self.refresh_image_space();
                self.refresh_door_fade();
            }
            "twf" => {
                self.console.wireframe = !self.console.wireframe;
                let mut server = godot::classes::RenderingServer::singleton();
                server.set_debug_generate_wireframes(true);
                let draw = if self.console.wireframe {
                    godot::classes::viewport::DebugDraw::WIREFRAME
                } else {
                    godot::classes::viewport::DebugDraw::DISABLED
                };
                self.base()
                    .get_viewport()
                    .ok_or("missing viewport")?
                    .set_debug_draw(draw);
            }
            "tg" | "tt" | "tws" => {
                self.game_streamer
                    .as_mut()
                    .ok_or("missing streamer")?
                    .bind_mut()
                    .toggle_console_geometry(name);
            }
            "tb" => {
                self.game_streamer
                    .as_mut()
                    .ok_or("missing streamer")?
                    .bind_mut()
                    .toggle_console_borders();
            }
            "tm" => {
                self.console.menus_visible = !self.console.menus_visible;
                let alpha = if self.console.menus_visible { 1.0 } else { 0.0 };
                for layer in self.base().get_children().iter_shared() {
                    if let Ok(layer) = layer.try_cast::<CanvasLayer>() {
                        for child in layer.get_children().iter_shared() {
                            if let Ok(mut control) = child.try_cast::<godot::classes::Control>() {
                                control.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, alpha));
                            }
                        }
                    }
                }
            }
            "tll" => {
                self.console.lod_visible = !self.console.lod_visible;
                if let Some(streamer) = self.game_streamer.as_mut() {
                    let mut streamer = streamer.bind_mut();
                    streamer.set_terrain_lod_visible(self.console.lod_visible);
                    streamer.set_object_lod_visible(self.console.lod_visible);
                    streamer.set_tree_lod_visible(self.console.lod_visible);
                }
            }
            "csb" => {
                if let Some(view) = self.hud_view.as_mut() {
                    view.trigger_screen_blood(0);
                }
            }
            _ => return Err(format!("presentation toggle {name} is not bridged")),
        }
        Ok(())
    }
}

impl PlayerNode {
    pub(super) fn player_control_enabled(&self, index: usize) -> bool {
        if self.console.free_camera.is_some() && matches!(index, 0 | 3) {
            return true;
        }
        self.game.as_ref().is_none_or(|game| {
            game.world()
                .player_control_flags
                .get(index)
                .copied()
                .unwrap_or(false)
        })
    }

    pub(super) fn console_free_camera_step(&mut self, delta: f64) -> bool {
        if self.console.free_camera.is_none() {
            return false;
        }
        let (dx, dy) = std::mem::take(&mut self.pending_look);
        self.rig.look(dx, dy);
        self.look_stick(delta);
        let input = self.gather();
        let direction =
            self.rig.look_direction() * input.move_y + self.rig.right_flat() * input.move_x;
        let Some(free) = self.console.free_camera.as_mut() else {
            return false;
        };
        free.position = free.position + direction * (self.console.free_speed * 40.0 * delta as f32);
        let position = free.position;
        let freeze = free.freeze;
        if !freeze {
            if let Some(game) = self.game.as_mut() {
                game.tick(delta);
            }
            self.sync_npc_grounding(delta as f32);
        }
        let eye = crate::camera::Eye {
            position: crate::units::SkyrimPos::new(position.x, position.y, position.z),
            yaw: self.rig.yaw(),
            pitch: self.rig.pitch(),
        };
        if let Some(camera) = self.camera.as_mut() {
            camera.set_global_transform(super::godot_transform(&eye));
        }
        true
    }
}

#[cfg(test)]
mod selection_tests {
    use super::selectable_reference;
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

    #[test]
    fn click_selection_accepts_references_without_building_terrain_or_base_forms() {
        let mut plugin = Plugin::new(FileHeader::default());
        for (object, tag) in [
            (0x800, *b"CELL"),
            (0x801, *b"LAND"),
            (0x802, *b"MISC"),
            (0x803, *b"REFR"),
        ] {
            plugin.push_top(Record::new(Tag(tag), object));
        }
        let mut order = LoadOrder::new();
        order.add("Selection.esm", plugin).unwrap();
        let mut world = soe_game::World::new(order);
        let id = |object| GlobalFormId { file: 0, object };
        for object in [0x800, 0x801, 0x802] {
            assert!(!selectable_reference(&world, id(object)));
            assert!(world.reference_ref(id(object)).is_none());
            world.reference(id(object));
            assert!(!selectable_reference(&world, id(object)));
        }
        assert!(selectable_reference(&world, id(0x803)));
        assert!(world.reference_ref(id(0x803)).is_none());
        let player = world.player();
        assert!(selectable_reference(&world, player));
    }
}
