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

impl PlayerNode {
    pub(super) fn create_console(&mut self) {
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
        let label = self
            .console
            .commands
            .selected()
            .and_then(|id| {
                let game = self.game.as_mut()?;
                Some(format!(
                    "{} ({:08X})",
                    game.world().name_of(id).unwrap_or_default(),
                    game.world().runtime_id(id)
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
        let body = hit.get("rid")?.try_to::<Rid>().ok()?;
        self.query_collision
            .reference_for_body(body)
            .or_else(|| self.game_streamer.as_ref()?.bind().reference_for_body(body))
            .or_else(|| self.actor_bodies.owners().get(&body).copied())
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
        }
        Ok(())
    }
}

impl PlayerNode {
    fn console_toggle_presentation(&mut self, name: &str) -> Result<(), String> {
        match name {
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
