// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand models from WEAP/WNAM, ARMA first-person models and MGEF casting art.
//! Field layouts follow local xEdit wbDefinitionsTES5.pas and UESP MGEF.
//! Instances use the animated weapon bones, following CommonLib's BipedAnim.

use godot::classes::{ArrayMesh, Material, RenderingServer};
use godot::prelude::*;
use soe_esm::{GlobalFormId, Tag};
use soe_game::combat::Hand;

use crate::data_source::DataSource;
use crate::godot_layer::array_mesh_from;

struct Model {
    instance: Rid,
    _mesh: Gd<ArrayMesh>,
    _material: Gd<Material>,
}

#[derive(Default)]
pub(super) struct Hands {
    paths: [Option<String>; 2],
    models: [Vec<Model>; 2],
    last_poses: [Option<Transform3D>; 2],
    visible: bool,
}

fn model_path(world: &soe_game::World, id: GlobalFormId) -> Option<String> {
    world
        .record(id)?
        .find(Tag(*b"MODL"))
        .map(soe_esm::cursor::zstring)
        .filter(|path| !path.is_empty())
}

pub(super) fn paths(
    world: &mut soe_game::World,
    actor: GlobalFormId,
    first_person: bool,
) -> [Option<String>; 2] {
    [Hand::Left, Hand::Right].map(|hand| {
        if let Some(spell) = world.hand_spell(actor, hand) {
            let definition = world.spell(spell).ok()?;
            for entry in &definition.effects {
                let effect = world.resolve_from(spell, entry.base_effect)?;
                let data = world.record(effect)?.find(Tag(*b"DATA"))?;
                let raw = u32::from_le_bytes(data.get(0x5c..0x60)?.try_into().ok()?);
                if let Some(art) = world.resolve_from(effect, raw) {
                    return model_path(world, art);
                }
            }
            return None;
        }
        if let Some(weapon) = world.hand_weapon(actor, hand) {
            if first_person {
                let record = world.record(weapon)?;
                if let Some(bytes) = record.find(Tag(*b"WNAM")) {
                    let raw = u32::from_le_bytes(bytes.try_into().ok()?);
                    if let Some(id) = world.resolve_from(weapon, raw) {
                        return model_path(world, id);
                    }
                }
            }
            return model_path(world, weapon);
        }
        if hand == Hand::Left && actor == world.player() {
            world.equipped_shield(actor).ok().flatten()?;
            let appearance = world.actor_appearance(actor).ok()?;
            let shield = appearance
                .parts
                .iter()
                .find(|part| part.visible_slots & (1 << 9) != 0)?;
            return if first_person {
                shield
                    .first_person_model
                    .clone()
                    .or_else(|| shield.model.clone())
            } else {
                shield.model.clone()
            };
        }
        None
    })
}

impl Hands {
    pub(super) fn sync(&mut self, source: &DataSource, paths: [Option<String>; 2], scenario: Rid) {
        for (index, path) in paths.into_iter().enumerate() {
            if self.paths[index] == path {
                continue;
            }
            self.clear_hand(index);
            self.paths[index] = path.clone();
            let Some(path) = path else { continue };
            match source.item_preview_surfaces(&path) {
                Ok(surfaces) => {
                    let mut server = RenderingServer::singleton();
                    for (arrays, textures) in surfaces {
                        let mut mesh = array_mesh_from(&arrays);
                        let material =
                            crate::godot_layer::material_from(&arrays.material, &textures);
                        mesh.surface_set_material(0, &material);
                        let instance = server.instance_create2(mesh.get_rid(), scenario);
                        self.models[index].push(Model {
                            instance,
                            _mesh: mesh,
                            _material: material,
                        });
                    }
                }
                Err(error) => {
                    soe_trace::error!(target: "host::actor", system = "host::actor", "hand model {path}: {error}");
                }
            }
        }
    }

    pub(super) fn pose(&mut self, poses: [Option<Transform3D>; 2], visible: bool) {
        self.last_poses = poses;
        self.visible = visible;
        let mut server = RenderingServer::singleton();
        for (models, pose) in self.models.iter().zip(poses) {
            for model in models {
                server.instance_set_visible(model.instance, visible && pose.is_some());
                if let Some(pose) = pose {
                    server.instance_set_transform(model.instance, pose);
                }
            }
        }
    }

    pub(super) fn observation(&self, bones: [Option<&str>; 2]) -> Array<VarDictionary> {
        let mut rows = Array::new();
        for (index, bone) in bones.into_iter().enumerate() {
            let Some(path) = &self.paths[index] else {
                continue;
            };
            let mut row = VarDictionary::new();
            row.set("left", index == 0);
            row.set("model", path.as_str());
            row.set("bone", bone.unwrap_or_default());
            row.set("shapes", self.models[index].len() as i64);
            row.set("visible", self.visible && self.last_poses[index].is_some());
            if let Some(pose) = self.last_poses[index] {
                row.set("pose", pose);
            }
            rows.push(&row);
        }
        rows
    }

    pub(super) fn count(&self) -> i64 {
        self.models.iter().map(Vec::len).sum::<usize>() as i64
    }

    fn clear_hand(&mut self, index: usize) {
        let mut server = RenderingServer::singleton();
        for model in self.models[index].drain(..) {
            server.free_rid(model.instance);
        }
    }

    pub(super) fn clear(&mut self) {
        self.clear_hand(0);
        self.clear_hand(1);
        self.paths = [None, None];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    #[test]
    fn hst_25_inventory_shield_selects_the_sex_specific_addon_in_both_views() {
        let id = |object| GlobalFormId { file: 0, object };
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"RACE"), 0x300));
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), 7)
                .zsub(Tag(*b"EDID"), "Player")
                .sub(Tag(*b"RNAM"), 0x300_u32.to_le_bytes()),
        );
        plugin.push_top(
            Record::new(Tag(*b"ARMA"), 0x301)
                .sub(Tag(*b"BOD2"), (1_u32 << 9).to_le_bytes())
                .sub(Tag(*b"RNAM"), 0x300_u32.to_le_bytes())
                .sub(Tag(*b"DNAM"), [0; 12])
                .zsub(Tag(*b"MOD2"), "armor/shield.nif")
                .zsub(Tag(*b"MOD4"), "armor/1stpersonshield.nif"),
        );
        plugin.push_top(
            Record::new(Tag(*b"ARMO"), 0x302)
                .sub(
                    Tag(*b"BOD2"),
                    [(1_u32 << 9).to_le_bytes(), 0_u32.to_le_bytes()].concat(),
                )
                .sub(Tag(*b"DNAM"), 100_u32.to_le_bytes())
                .sub(Tag(*b"MODL"), 0x301_u32.to_le_bytes()),
        );
        let mut order = LoadOrder::new();
        order.add("Shield.esm", plugin).unwrap();
        let mut game = soe_game::Game::new(order);
        let player = game.player();
        game.world_mut().add_item(player, id(0x302), 1);
        assert!(crate::inventory_menu::toggle_equip(&mut game, id(0x302)));
        let mut world = game.world_mut();
        let appearance = world.actor_appearance(player).unwrap();
        assert!(
            !appearance.parts.is_empty(),
            "fixture must resolve its equipped addon: {appearance:?}"
        );
        assert_eq!(
            paths(&mut world, player, false)[0].as_deref(),
            Some("armor/shield.nif")
        );
        assert_eq!(
            paths(&mut world, player, true)[0].as_deref(),
            Some("armor/1stpersonshield.nif")
        );
    }
}
