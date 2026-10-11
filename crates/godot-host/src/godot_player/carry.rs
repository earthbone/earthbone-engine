// SPDX-License-Identifier: GPL-3.0-or-later

//! Player carry input over resident server bodies. CK OnGrab and OnRelease
//! have no parameters. Godot's first ray hit preserves obstruction ordering.

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
}

impl PlayerNode {
    fn host_activate_hit(&mut self) -> Option<(GlobalFormId, Vector3)> {
        let space = self.base().get_world_3d()?.get_space();
        let eye = self.rig.head(self.feet(), self.capsule_height());
        let end = eye + self.rig.look_direction() * super::ACTIVATE_REACH;
        let convert = |v: soe_physics::Vec3| {
            let p = SkyrimPos::new(v.x, v.y, v.z).to_godot();
            Vector3::new(p.x, p.y, p.z)
        };
        let mut query = PhysicsRayQueryParameters3D::new_gd();
        query.set_from(convert(eye));
        query.set_to(convert(end));
        query.set_hit_from_inside(true);
        let player = self.game.as_mut().map(soe_game::Game::player);
        let excluded: Array<Rid> = self
            .actor_bodies
            .owners()
            .iter()
            .filter_map(|(rid, owner)| (Some(*owner) == player).then_some(*rid))
            .collect();
        query.set_exclude(&excluded);
        let mut state = PhysicsServer3D::singleton().space_get_direct_state(space)?;
        soe_trace::count(soe_trace::Operation::Raycasts, 1);
        let hit = state.intersect_ray(&query);
        let body = hit.get("rid")?.try_to::<Rid>().ok()?;
        let point = hit.get("position")?.try_to::<Vector3>().ok()?;
        let reference = self
            .query_collision
            .reference_for_body(body)
            .or_else(|| self.game_streamer.as_ref()?.bind().reference_for_body(body))
            .or_else(|| self.actor_bodies.owners().get(&body).copied())?;
        Some((reference, point))
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
        let gesture = self
            .activate_hold
            .step(input.activate_pressed, held, delta, target);
        if self.grabbed_host.is_some() && !held {
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
                    });
                    if let Some(game) = &mut self.game {
                        game.world_mut().set_grabbed_reference(Some(reference));
                    }
                    self.carry_event(reference, "OnGrab");
                } else {
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
        let applied = enabled
            && self.game_streamer.as_ref().is_some_and(|streamer| {
                streamer.bind().spring_carry(
                    carry.reference,
                    carry.anchor,
                    Vector3::new(target.x, target.y, target.z),
                    delta as f32,
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
