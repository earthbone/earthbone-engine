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

    pub(super) fn pose(&self, poses: [Option<Transform3D>; 2], visible: bool) {
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
