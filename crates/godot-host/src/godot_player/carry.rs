// SPDX-License-Identifier: GPL-3.0-or-later

//! Player carry input over resident server bodies. CK OnGrab and OnRelease
//! have no parameters. Rendered door geometry refines static collision proxies.

use godot::classes::{PhysicsRayQueryParameters3D, PhysicsServer3D};
use godot::prelude::*;
use soe_esm::GlobalFormId;

use super::PlayerNode;
use crate::carry::ActivateGesture;
use crate::input::{Action, InputState};
use crate::units::{GodotPos, SkyrimPos};

pub(super) struct HeldReference {
    reference: GlobalFormId,
    anchor: Vector3,
    distance: f32,
    toggled: bool,
}

impl PlayerNode {
    fn host_activate_hit(&mut self) -> Option<(GlobalFormId, Vector3)> {
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let end = eye + self.rig.look_direction() * super::ACTIVATE_REACH;
        let convert = |v: soe_physics::Vec3| {
            let p = SkyrimPos::new(v.x, v.y, v.z).to_godot();
            Vector3::new(p.x, p.y, p.z)
        };
        self.host_reference_ray(convert(eye), convert(end))
    }

    pub(super) fn host_reference_ray(
        &mut self,
        from: Vector3,
        to: Vector3,
    ) -> Option<(GlobalFormId, Vector3)> {
        let space = self.base().get_world_3d()?.get_space();
        let mut query = PhysicsRayQueryParameters3D::new_gd();
        query.set_from(from);
        query.set_to(to);
        query.set_hit_from_inside(true);
        let player = self.game.as_mut().map(soe_game::Game::player);
        let mut excluded: Array<Rid> = self
            .actor_bodies
            .owners()
            .iter()
            .filter_map(|(rid, owner)| (Some(*owner) == player).then_some(*rid))
            .collect();
        query.set_exclude(&excluded);
        let mut state = PhysicsServer3D::singleton().space_get_direct_state(space)?;
        soe_trace::count(soe_trace::Operation::Raycasts, 1);
        let hit = state.intersect_ray(&query);
        let physical = (|| {
            let body = hit.get("rid")?.try_to::<Rid>().ok()?;
            let point = hit.get("position")?.try_to::<Vector3>().ok()?;
            let reference = self.ray_body_reference(body)?;
            Some((reference, point))
        })();
        let refines_static = physical.map_or_else(
            || hit.is_empty(),
            |(reference, _)| self.ray_static_proxy(reference),
        );
        if refines_static {
            if let Some(rendered) = self
                .game_streamer
                .as_ref()
                .and_then(|streamer| streamer.bind().rendered_door_ray(from, to))
            {
                query.set_to(rendered.1);
                let mut blocker = hit;
                while !blocker.is_empty() {
                    let body = blocker.get("rid")?.try_to::<Rid>().ok()?;
                    let reference = self.ray_body_reference(body)?;
                    if !self.ray_static_proxy(reference) {
                        let point = blocker.get("position")?.try_to::<Vector3>().ok()?;
                        return Some((reference, point));
                    }
                    if excluded.contains(body) {
                        return physical;
                    }
                    excluded.push(body);
                    query.set_exclude(&excluded);
                    soe_trace::count(soe_trace::Operation::Raycasts, 1);
                    blocker = state.intersect_ray(&query);
                }
                return Some(rendered);
            }
        }
        physical
    }

    fn ray_body_reference(&self, body: Rid) -> Option<GlobalFormId> {
        self.query_collision
            .reference_for_body(body)
            .or_else(|| self.game_streamer.as_ref()?.bind().reference_for_body(body))
            .or_else(|| self.actor_bodies.owners().get(&body).copied())
            .or_else(|| self.npc_actors.console_reference_for_body(body))
    }

    fn ray_static_proxy(&mut self, reference: GlobalFormId) -> bool {
        self.game.as_mut().is_some_and(|game| {
            let mut world = game.world_mut();
            world
                .base_of(reference)
                .and_then(|base| world.record(base))
                .is_some_and(|record| matches!(record.tag.as_bytes(), b"STAT" | b"MSTT" | b"DOOR"))
        })
    }

    pub(super) fn update_carry_input(&mut self, input: &InputState, delta: f64) {
        let hit = self.host_activate_hit();
        let target = hit.map(|(reference, _)| reference);
        if let Some(game) = &mut self.game {
            game.world_mut().set_crosshair_reference(target);
        }
        self.last_activate_hit = false;
        self.last_activate_reference = None;
        if self
            .game
            .as_ref()
            .is_none_or(|game| !game.world().player_control_flags[6])
        {
            self.release_carry();
            return;
        }
        if self.host_telekinesis_target.is_some() {
            self.activate_hold.cancel();
            return;
        }
        let held = self.held_actions.contains(&Action::Activate);
        if self.grabbed_host.is_some() && input.right_attack_pressed {
            let direction = self.rig.look_direction();
            let godot_direction =
                crate::units::direction_to_godot([direction.x, direction.y, direction.z]);
            if let (Some(carry), Some(streamer)) = (&self.grabbed_host, &self.game_streamer) {
                streamer
                    .bind()
                    .throw_carried_reference(carry.reference, Vector3::from_array(godot_direction));
            }
            self.release_carry();
            return;
        }
        if self.grabbed_host.is_some() && (input.grab_pressed || input.activate_pressed) {
            self.release_carry();
            return;
        }
        let gesture = if input.grab_pressed {
            self.activate_hold.cancel();
            target.map_or(ActivateGesture::None, ActivateGesture::Grab)
        } else {
            self.activate_hold
                .step(input.activate_pressed, held, delta, target)
        };
        if self
            .grabbed_host
            .as_ref()
            .is_some_and(|carry| !carry.toggled && !held)
        {
            self.release_carry();
            return;
        }
        match gesture {
            ActivateGesture::Tap(reference) => {
                self.last_activate_hit = true;
                self.last_activate_reference = Some(reference);
            }
            ActivateGesture::Grab(reference) => {
                let anchor = hit.and_then(|(_, point)| {
                    self.game_streamer
                        .as_ref()?
                        .bind()
                        .carry_anchor(reference, point)
                });
                if let (Some(anchor), Some((_, point))) = (anchor, hit) {
                    let eye = self.rig.head(self.feet(), self.capsule_height());
                    let point = GodotPos::new(point.x, point.y, point.z).to_skyrim();
                    let distance =
                        (soe_physics::Vec3::new(point.x, point.y, point.z) - eye).length();
                    self.grabbed_host = Some(HeldReference {
                        reference,
                        anchor,
                        distance,
                        toggled: input.grab_pressed,
                    });
                    if let Some(game) = &mut self.game {
                        game.world_mut().set_grabbed_reference(Some(reference));
                    }
                    self.carry_event(reference, "OnGrab");
                } else if !input.grab_pressed {
                    self.last_activate_hit = true;
                    self.last_activate_reference = Some(reference);
                }
            }
            ActivateGesture::None => {}
        }
        let Some(carry) = &self.grabbed_host else {
            return;
        };
        let enabled = self
            .game
            .as_ref()
            .is_some_and(|game| game.world().reference_enabled(carry.reference));
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let target = eye + self.rig.look_direction() * carry.distance;
        let target = SkyrimPos::new(target.x, target.y, target.z).to_godot();
        let max_force = self
            .game
            .as_ref()
            .map_or(0.0, |game| crate::carry::max_force(&game.world()));
        let applied = enabled
            && self.game_streamer.as_ref().is_some_and(|streamer| {
                streamer.bind().spring_carry(
                    carry.reference,
                    carry.anchor,
                    Vector3::new(target.x, target.y, target.z),
                    delta as f32,
                    max_force,
                )
            });
        if !applied {
            self.release_carry();
        }
    }

    pub(super) fn release_carry(&mut self) {
        self.activate_hold.cancel();
        if let Some(held) = self.grabbed_host.take() {
            if let Some(game) = &mut self.game {
                game.world_mut().set_grabbed_reference(None);
            }
            self.carry_event(held.reference, "OnRelease");
        }
    }

    fn carry_event(&mut self, reference: GlobalFormId, name: &str) {
        if let Some(game) = &mut self.game {
            queue_carry_event(game, reference, name);
        }
    }
}

fn queue_carry_event(game: &mut soe_game::Game, reference: GlobalFormId, name: &str) {
    let world = game.shared_world();
    world
        .borrow_mut()
        .form_event(reference, name, Vec::new(), &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hst_22_grab_release_events_target_reference_with_no_arguments() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        plugin.push_top(soe_esm::Record::new(soe_esm::Tag::new(b"MISC"), 42));
        plugin.push_top(
            soe_esm::Record::new(soe_esm::tag::REFR, 43)
                .sub(b"NAME", 42_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let mut game = soe_game::Game::new(order);
        let reference = GlobalFormId {
            file: 0,
            object: 43,
        };
        let object = game.object_of_form(reference);
        game.world_mut().pending.clear();
        for name in ["OnGrab", "OnRelease"] {
            queue_carry_event(&mut game, reference, name);
        }
        let world = game.world();
        assert_eq!(world.pending.len(), 2);
        for (event, expected) in world.pending.iter().zip(["OnGrab", "OnRelease"]) {
            assert!(
                matches!(event, soe_game::world::Pending::Event { object: target, name, args } if *target == object && name == expected && args.is_empty())
            );
        }
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Current shared crosshair and grabbed references for host diagnostics.
    #[func]
    pub fn carry_status(&self) -> VarDictionary {
        let mut row = VarDictionary::new();
        if let Some(game) = &self.game {
            let world = game.world();
            for (name, reference) in [
                ("crosshair", world.crosshair_reference()),
                ("grabbed", world.grabbed_reference()),
            ] {
                if let Some(reference) = reference {
                    row.set(format!("{name}_file"), i64::from(reference.file));
                    row.set(format!("{name}_object"), i64::from(reference.object));
                }
            }
        }
        if let Some(carry) = &self.grabbed_host {
            let eye = self.rig.head(self.feet(), self.capsule_height());
            let target = eye + self.rig.look_direction() * carry.distance;
            let target = SkyrimPos::new(target.x, target.y, target.z).to_godot();
            row.set("hold_point", Vector3::new(target.x, target.y, target.z));
            row.set("anchor", carry.anchor);
        }
        row
    }

    /// Read a host reference through the corresponding Papyrus Game native.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn debug_reference_native(&mut self, name: GString) -> VarDictionary {
        let name = name.to_string();
        if !matches!(
            name.as_str(),
            "GetCurrentCrosshairRef" | "GetPlayerGrabbedRef"
        ) {
            return VarDictionary::new();
        }
        let script_name = "SoeHst18ReferenceCheck";
        let function = soe_papyrus::asm::FunctionBuilder::new("Read", "Form")
            .global()
            .local("result", "Form")
            .ins(
                soe_papyrus::Opcode::CallStatic,
                vec![
                    soe_papyrus::asm::s("Game"),
                    soe_papyrus::asm::s(&name),
                    soe_papyrus::asm::ident("result"),
                ],
            )
            .ins(
                soe_papyrus::Opcode::Return,
                vec![soe_papyrus::asm::ident("result")],
            )
            .build();
        let bytes = soe_papyrus::asm::ScriptBuilder::new(script_name, "Form")
            .function(function)
            .bytes();
        let Some(game) = self.game.as_mut() else {
            return VarDictionary::new();
        };
        if game.load_script(&bytes).is_err() {
            return VarDictionary::new();
        }
        let Ok(thread) = game.vm.call_global(script_name, "Read", &[]) else {
            return VarDictionary::new();
        };
        if !game.run_until_idle(64) {
            return VarDictionary::new();
        }
        let Some(soe_papyrus::Value::Object(object)) = game.vm.thread_result(thread) else {
            return VarDictionary::new();
        };
        let Some(reference) = game.world().form_of(object) else {
            return VarDictionary::new();
        };
        let mut row = VarDictionary::new();
        row.set("file", i64::from(reference.file));
        row.set("object", i64::from(reference.object));
        row
    }
}
