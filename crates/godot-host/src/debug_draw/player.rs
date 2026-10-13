// SPDX-License-Identifier: GPL-3.0-or-later

//! PlayerNode bridge for console view requests. The console dispatch owns names,
//! selection and switches. Godot resolves only live rendering and camera data.

use godot::classes::{Camera3D, ProjectSettings, RenderingServer};
use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::debug::Outcome;
use soe_game::debug_views::HostRequest;

use super::PlayerNode;

impl PlayerNode {
    pub(super) fn refresh_debug_views(&mut self) {
        let drawing = self
            .game
            .as_ref()
            .is_some_and(|game| game.debug_views.drawing());
        if drawing {
            let mut parent = self.base().clone();
            if let Some(game) = self.game.as_mut() {
                self.debug_draw.update(&mut parent, game);
            }
        } else if !self.debug_draw.counts.is_empty() {
            self.debug_draw.clear();
        }
        let lines = self
            .game
            .as_mut()
            .map(soe_game::debug_views::papyrus_lines)
            .unwrap_or_default();
        for line in lines {
            self.signals()
                .debug_view_message()
                .emit(&GString::from(&line));
        }
    }

    /// Resolve requests made by any Console session against this node's Game.
    /// The shared console overlay calls this after Console::run, preserving its
    /// own selected reference, history and playable-command dispatch.
    pub fn finish_debug_views(&mut self, outcome: Result<Outcome, String>) -> GString {
        let mut lines = vec![match outcome {
            Ok(outcome) => outcome.to_string(),
            Err(error) => format!("error: {error}"),
        }];
        let requests: Vec<_> = self
            .game
            .as_mut()
            .map(|game| game.debug_views.host_requests.drain(..).collect())
            .unwrap_or_default();
        for request in requests {
            match self.debug_host_request(request) {
                Ok(report) => lines.extend(report),
                Err(error) => lines.push(format!("error: {error}")),
            }
        }
        self.refresh_debug_views();
        GString::from(&lines.join("\n"))
    }

    fn debug_host_request(&mut self, request: HostRequest) -> Result<Vec<String>, String> {
        match request {
            HostRequest::Animation(actor) => {
                let mut node = self
                    .debug_actor_node(actor)
                    .ok_or_else(|| format!("{actor}: no resident animation node"))?;
                let report = node.bind_mut().debug_animation_report();
                Ok(report)
            }
            HostRequest::Scenegraph(reference) => self.debug_scenegraph(reference),
            HostRequest::MapCamera => Ok(vec![self.toggle_debug_map()]),
            HostRequest::LocalMap => {
                let game = self.game.as_mut().ok_or("no game")?;
                game.debug_views.navmesh = true;
                if self.debug_map_camera.is_none() {
                    self.toggle_debug_map();
                }
                Ok(vec!["local map: loaded NAVM, fog of war off".into()])
            }
            HostRequest::Hdr => {
                let mut viewport = self.base().get_viewport().ok_or("no viewport")?;
                if let Some(previous) = self.debug_hdr_restore.take() {
                    viewport.set_debug_draw(previous);
                    return Ok(vec!["HDR luminance debug off".into()]);
                }
                if RenderingServer::singleton().get_current_rendering_method() == "gl_compatibility"
                {
                    return Err("HDR luminance debug needs Godot Forward+ or Mobile".into());
                }
                self.debug_hdr_restore = Some(viewport.get_debug_draw());
                viewport.set_debug_draw(godot::classes::viewport::DebugDraw::SCENE_LUMINANCE);
                Ok(vec!["HDR luminance debug on".into()])
            }
        }
    }

    fn debug_actor_node(
        &self,
        actor: GlobalFormId,
    ) -> Option<Gd<crate::godot_actor::SkinnedActorNode>> {
        self.npc_actors.debug_actor(actor).or_else(|| {
            (actor.file == 0 && actor.object == 0x14)
                .then(|| self.player_appearance.clone())
                .flatten()
        })
    }

    fn toggle_debug_map(&mut self) -> String {
        if let Some(mut camera) = self.debug_map_camera.take() {
            camera.queue_free();
            if let Some(camera) = self.camera.as_mut() {
                camera.make_current();
            }
            return "map camera off".into();
        }
        let mut camera = Camera3D::new_alloc();
        camera.set_name("SoeDebugMapCamera");
        camera.set_projection(godot::classes::camera_3d::ProjectionType::ORTHOGONAL);
        camera.set_size(60.0);
        camera.set_far(1000.0);
        camera.set_cull_mask((1 << 20) - 1);
        self.base_mut().add_child(&camera);
        camera.set_as_top_level(true);
        let position = self
            .camera
            .as_ref()
            .map_or(Vector3::ZERO, |camera| camera.get_global_position());
        camera.set_global_position(position + Vector3::UP * 100.0);
        camera.set_global_rotation(Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0));
        camera.make_current();
        self.debug_map_camera = Some(camera);
        "map camera on".into()
    }

    fn debug_scenegraph(&self, reference: GlobalFormId) -> Result<Vec<String>, String> {
        let game = self.game.as_ref().ok_or("no game")?;
        let source = self.data_source.as_ref().ok_or("no data source")?;
        let mut world = game.world_mut();
        let placement = world
            .ref_placement(reference)
            .ok_or("no placed reference")?;
        let mut lines = vec![format!("reference {reference} live_transform={placement:?} attached_3d={} collision=host PhysicsServer/SimpleWorld", world.is_reference_3d_loaded(reference))];
        lines.push(format!(
            "host_static_collision_bodies={:?} host_static_collision_enabled={:?}",
            self.collision_bodies.get(&reference),
            self.collision_visibility.get(&reference)
        ));
        let capsule = if reference.file == 0 && reference.object == 0x14 {
            self.character()
        } else {
            self.world
                .as_ref()
                .and_then(|physics| self.npc_grounding.character(physics, reference))
        };
        if let Some(capsule) = capsule {
            lines.push(format!(
                "host_capsule radius={} height={} position={:?} grounded={}",
                capsule.settings.radius,
                capsule.height(),
                capsule.position,
                capsule.on_ground
            ));
        } else {
            lines.push("host_capsule absent".into());
        }
        if let Some(state) = world.reference_ref(reference) {
            lines.push(format!(
                "opened={} locked={} lock_level={} key={:?} teleport={:?}",
                state.opened,
                state.locked,
                state.lock_level,
                world.key(reference),
                state.teleport
            ));
        }
        let model = if let Some(node) = self.debug_actor_node(reference) {
            let node = node.bind();
            lines.push(format!(
                "host actor {} shapes={:?} skin_poses={:?}",
                node.node_identity(),
                node.stats(),
                node.uploaded_skin_poses()
            ));
            node.debug_nif_path()
                .ok_or("actor has no NIF skeleton model")?
        } else {
            world
                .base_of(reference)
                .and_then(|base| world.record(base))
                .and_then(crate::data_source::model_for)
                .ok_or("reference has no model")?
        };
        let bytes = source
            .vfs()
            .read(&model)
            .map_err(|error| format!("{model}: {error}"))?;
        let file = soe_nif::parse(&bytes).map_err(|error| format!("{model}: {error}"))?;
        lines.push(format!("model {model}"));
        lines.extend(crate::debug_draw::scenegraph::report(&file));
        Ok(lines)
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Standalone diagnostic Console session for walkthroughs and host tests.
    /// A production console can instead pass its outcome to finish_debug_views.
    #[func]
    pub fn debug_view_command(&mut self, line: GString) -> GString {
        if self.simulation.is_some() {
            return "error: debug views require inline gameplay".into();
        }
        let Some(game) = self.game.as_mut() else {
            return "error: no game".into();
        };
        if game.debug_views.papyrus_directory.is_none() {
            game.debug_views.papyrus_directory = Some(std::path::PathBuf::from(
                ProjectSettings::singleton()
                    .globalize_path("user://debug-views")
                    .to_string(),
            ));
        }
        let command = String::from(line);
        let outcome = self.debug_view_console.run(game, &command);
        let output = self.finish_debug_views(outcome);
        self.signals().debug_view_message().emit(&output);
        output
    }

    /// Counts of primitives uploaded to the current optional debug nodes.
    #[func]
    pub fn debug_geometry_counts(&self) -> VarDictionary {
        self.debug_draw.counts.clone()
    }
}
