// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-23 derived characters use winning records and retained World state.

use crate::common::installed;
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{appearance::ActorAppearance, npc, Game};
use std::collections::BTreeSet;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn head(object: u32, kind: u32, model: &str) -> Record {
    Record::new(T(b"HDPT"), object)
        .sub(T(b"PNAM"), kind.to_le_bytes())
        .zsub(T(b"MODL"), model)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"RACE"), 0x800)
            .sub(T(b"WNAM"), 0x810u32.to_le_bytes())
            .sub(T(b"MNAM"), [])
            .sub(T(b"HEAD"), 0x820u32.to_le_bytes())
            .sub(T(b"FNAM"), [])
            .sub(T(b"HEAD"), 0x821u32.to_le_bytes()),
        Record::new(T(b"ARMO"), 0x810)
            .sub(T(b"DNAM"), 0u32.to_le_bytes())
            .sub(T(b"BOD2"), (1u32 << 2).to_le_bytes())
            .sub(T(b"MODL"), 0x811u32.to_le_bytes()),
        Record::new(T(b"ARMA"), 0x811)
            .sub(T(b"BOD2"), (1u32 << 2).to_le_bytes())
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"RNAM"), 0x800u32.to_le_bytes())
            .zsub(T(b"MOD2"), "body_m.nif")
            .zsub(T(b"MOD3"), "body_f.nif"),
        Record::new(T(b"ARMO"), 0x812)
            .sub(T(b"DNAM"), 0u32.to_le_bytes())
            .sub(T(b"BOD2"), ((1u32 << 2) | (1 << 1)).to_le_bytes())
            .sub(T(b"MODL"), 0x813u32.to_le_bytes()),
        Record::new(T(b"ARMA"), 0x813)
            .sub(T(b"BOD2"), ((1u32 << 2) | (1 << 1)).to_le_bytes())
            .sub(T(b"DNAM"), [10; 12])
            .sub(T(b"RNAM"), 0x800u32.to_le_bytes())
            .zsub(T(b"MOD2"), "outfit_m.nif")
            .zsub(T(b"MOD3"), "outfit_f.nif"),
        Record::new(T(b"ARMO"), 0x815)
            .sub(T(b"DNAM"), 0u32.to_le_bytes())
            .sub(T(b"BOD2"), ((1u32 << 2) | (1 << 7)).to_le_bytes())
            .sub(T(b"MODL"), 0x816u32.to_le_bytes()),
        Record::new(T(b"ARMA"), 0x816)
            .sub(T(b"BOD2"), ((1u32 << 2) | (1 << 7)).to_le_bytes())
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"RNAM"), 0x800u32.to_le_bytes())
            .zsub(T(b"MOD2"), "distributed_m.nif")
            .zsub(T(b"MOD3"), "distributed_f.nif"),
        Record::new(T(b"OTFT"), 0x814).sub(T(b"INAM"), 0x812u32.to_le_bytes()),
        head(0x820, 1, "race_m.nif"),
        head(0x821, 1, "race_f.nif"),
        head(0x822, 1, "npc_head.nif").sub(T(b"HNAM"), 0x823u32.to_le_bytes()),
        head(0x823, 0, "eyes_extra.nif").sub(T(b"HNAM"), 0x822u32.to_le_bytes()),
        head(0x824, 3, "hair.nif"),
        npc::build_record(
            0x830,
            "Character",
            "Character",
            npc::Config {
                flags: npc::flags::FEMALE,
                ..Default::default()
            },
            npc::Stats::default(),
        )
        .sub(T(b"RNAM"), 0x800u32.to_le_bytes())
        .sub(T(b"DOFT"), 0x814u32.to_le_bytes())
        .sub(T(b"PNAM"), 0x822u32.to_le_bytes())
        .sub(T(b"PNAM"), 0x824u32.to_le_bytes()),
    ] {
        plugin.push_top(record);
    }
    for cell_id in [0x840, 0x841] {
        let cell = plugin.push_record(Record::new(T(b"CELL"), cell_id).sub(T(b"DATA"), [1, 0]));
        let reference = plugin.push_record(
            Record::new(T(b"ACHR"), cell_id + 0x10)
                .sub(T(b"NAME"), 0x830u32.to_le_bytes())
                .sub(T(b"DATA"), [0; 24]),
        );
        let mut temporary = Group::new(GroupKind::CellTemporary(cell_id));
        temporary.entries.push(Entry::Record(reference));
        if cell_id == 0x840 {
            for (object, parent) in [
                (0x852, None),
                (0x853, Some((0x852u32, 0u32))),
                (0x854, Some((0x853u32, 1u32))),
            ] {
                let mut record = Record::new(T(b"ACHR"), object)
                    .sub(T(b"NAME"), 0x830u32.to_le_bytes())
                    .sub(T(b"DATA"), [0; 24]);
                if let Some((parent, flags)) = parent {
                    let mut link = parent.to_le_bytes().to_vec();
                    link.extend_from_slice(&flags.to_le_bytes());
                    record = record.sub(T(b"XESP"), link);
                } else {
                    record.flags = soe_esm::plugin::flags::INITIALLY_DISABLED;
                }
                let reference = plugin.push_record(record);
                temporary.entries.push(Entry::Record(reference));
            }
        }
        let mut children = Group::new(GroupKind::CellChildren(cell_id));
        children.entries.push(Entry::Group(temporary));
        let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
        sub.entries
            .extend([Entry::Record(cell), Entry::Group(children)]);
        let mut block = Group::new(GroupKind::InteriorBlock(0));
        block.entries.push(Entry::Group(sub));
        let mut top = Group::new(GroupKind::Top(T(b"CELL")));
        top.entries.push(Entry::Group(block));
        plugin.groups.push(top);
    }
    let mut order = LoadOrder::new();
    order
        .add("Characters.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

#[test]
fn hst_23_outfit_sex_head_extras_and_covered_hair_come_from_records() {
    let mut game = fixture();
    game.change_cell(Some(id(0x840)));
    let appearance = game.world_mut().actor_appearance(id(0x850)).unwrap();
    assert!(appearance.female);
    assert_eq!(
        appearance
            .parts
            .iter()
            .map(|part| part.model.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["outfit_f.nif"]
    );
    assert_eq!(
        appearance
            .head_parts
            .iter()
            .map(|part| part.form)
            .collect::<Vec<_>>(),
        [id(0x822), id(0x823)]
    );
    assert_eq!(appearance.skin, Some(id(0x810)));
    let face = appearance.facegen.unwrap();
    assert_eq!(
        (face.plugin.as_str(), face.object),
        ("Characters.esm", 0x830)
    );
}

#[test]
fn hst_23_npc_heads_override_sex_specific_race_defaults() {
    let game = fixture();
    let world = game.world();
    let appearance =
        ActorAppearance::from_order(world.order(), None, Some(id(0x800)), &BTreeSet::new())
            .unwrap();
    assert_eq!(appearance.head_parts[0].form, id(0x820));
    let appearance =
        ActorAppearance::from_order(world.order(), Some(id(0x830)), None, &BTreeSet::new())
            .unwrap();
    assert_eq!(
        appearance
            .head_parts
            .iter()
            .map(|part| part.form)
            .collect::<Vec<_>>(),
        [id(0x822), id(0x823), id(0x824)]
    );
}

#[test]
fn hst_23_malformed_head_link_is_a_typed_error() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x830).sub(T(b"PNAM"), [0; 3]));
    let mut order = LoadOrder::new();
    order.add("Bad.esm", plugin).unwrap();
    assert!(ActorAppearance::from_order(&order, Some(id(0x830)), None, &BTreeSet::new()).is_err());
}

#[test]
fn hst_23_disabled_unloaded_and_returning_characters_use_retained_runtime_state() {
    let mut game = fixture();
    game.change_cell(Some(id(0x840)));
    game.world_mut().reference(id(0x850)).unwrap().position = [100.0, 200.0, 300.0];
    game.world_mut().actor(id(0x850)).unwrap().equipped.clear();
    game.world_mut().set_reference_enabled(id(0x850), false);
    assert!(game
        .world_mut()
        .contact_actors()
        .iter()
        .all(|actor| actor.actor != id(0x850)));
    game.world_mut().set_reference_enabled(id(0x850), true);
    assert_eq!(
        game.world_mut()
            .contact_actors()
            .iter()
            .find(|actor| actor.actor == id(0x850))
            .unwrap()
            .position,
        [100.0, 200.0, 300.0]
    );
    game.change_cell(Some(id(0x841)));
    game.world_mut().settle_unload();
    assert!(game
        .world_mut()
        .contact_actors()
        .iter()
        .all(|actor| actor.actor != id(0x850)));
    game.change_cell(Some(id(0x840)));
    assert_eq!(
        game.world().ref_placement(id(0x850)).unwrap().position,
        [100.0, 200.0, 300.0]
    );
    assert!(game
        .world_mut()
        .actor(id(0x850))
        .unwrap()
        .equipped
        .is_empty());
}

#[test]
fn hst_23_installed_hulda_and_whiterun_guards_have_rigs_and_appearance() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let inn = game.form("WhiterunBanneredMare").unwrap();
    for exterior in [false, true] {
        let name = if exterior {
            "Whiterun outside Bannered Mare"
        } else {
            "WhiterunBanneredMare"
        };
        let cell = if exterior {
            let references = {
                let world = game.world();
                world.order().build_cell_index().references(inn).to_vec()
            };
            let exit = references
                .iter()
                .find_map(|id| {
                    game.world_mut()
                        .reference(*id)
                        .and_then(|reference| reference.teleport.map(|(door, _)| door))
                })
                .expect("Bannered Mare exit destination");
            let (parent, position) = {
                let mut world = game.world_mut();
                let reference = world.reference(exit).unwrap();
                (reference.cell.unwrap(), reference.position)
            };
            let worldspace = game.world().cell_coordinate_space(parent);
            game.world()
                .order()
                .build_cell_index()
                .exterior_cell(
                    worldspace,
                    (position[0] / 4096.0).floor() as i32,
                    (position[1] / 4096.0).floor() as i32,
                )
                .expect("grid outside Bannered Mare")
        } else {
            inn
        };
        game.change_cell(Some(cell));
        if exterior {
            let world = game.world();
            let index = world.order().build_cell_index();
            let grid = index.grid_of(cell).unwrap();
            let mut neighbors: Vec<_> = (-2..=2)
                .flat_map(|x| (-2..=2).map(move |y| (x, y)))
                .filter_map(|(x, y)| index.exterior_cell(grid.world, grid.x + x, grid.y + y))
                .filter(|id| *id != cell)
                .collect();
            neighbors.extend(index.world_persistent_cell(grid.world));
            drop(world);
            game.attach_streaming_cells(&neighbors).unwrap();
        }
        let actors = game.world_mut().contact_actors();
        assert!(!actors.is_empty(), "{name} has enabled actors");
        let mut found = false;
        for actor in actors {
            let mut world = game.world_mut();
            if actor.actor == world.player() {
                continue;
            }
            let base = world.base_of(actor.actor).unwrap();
            let label = world.npc(base).unwrap().editor_id.unwrap_or_default();
            let view = world.actor_appearance(actor.actor).unwrap();
            assert!(!view.parts.is_empty(), "{label} has body or outfit");
            assert!(world.actor_animation_rig(actor.actor).unwrap().is_some());
            if (!exterior && label.contains("Hulda")) || (exterior && label.contains("Guard")) {
                assert!(!view.head_parts.is_empty(), "{label} has head parts");
                found = true;
            }
        }
        assert!(found, "{name} contains Hulda or a guard");
    }
}

#[test]
fn hst_23_distributed_skin_keeps_outfit_and_head_selection() {
    let directory = std::env::temp_dir().join(format!("soe-hst-23-skin-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("HST23_DISTR.ini"),
        "Skin = 0x815~Characters.esm|Character\n",
    )
    .unwrap();
    let mut game = fixture();
    assert_eq!(
        game.apply_mod_data(std::slice::from_ref(&directory))
            .distributions_applied,
        1
    );
    game.change_cell(Some(id(0x840)));
    let view = game.world_mut().actor_appearance(id(0x850)).unwrap();
    assert_eq!(view.skin, Some(id(0x815)));
    assert!(view.parts.iter().any(|part| part.armor == id(0x812)));
    let skin = view
        .parts
        .iter()
        .find(|part| part.armor == id(0x815))
        .unwrap();
    assert_eq!(skin.model.as_deref(), Some("distributed_f.nif"));
    assert_eq!(skin.visible_slots, 1 << 7);
    assert_eq!(view.head_parts[0].form, id(0x822));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn hst_23_worker_snapshot_contains_the_runtime_appearance_and_pose() {
    use soe_game::simulation::{Config, Simulation};
    use std::time::{Duration, Instant};
    let worker = Simulation::spawn(Config::default(), || {
        let mut game = fixture();
        game.change_cell(Some(id(0x840)));
        game.world_mut().reference(id(0x850)).unwrap().position = [100.0, 200.0, 300.0];
        Ok(game)
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let snapshot = loop {
        if let Some(snapshot) = worker.poll() {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "worker published a snapshot");
        assert!(worker.failure().is_none());
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        snapshot.current.actors[&id(0x850)].position,
        [100.0, 200.0, 300.0]
    );
    assert_eq!(
        snapshot.current.appearances[&id(0x850)].parts[0]
            .model
            .as_deref(),
        Some("outfit_f.nif")
    );
    assert_eq!(
        snapshot.current.appearances[&id(0x850)].head_parts[0].form,
        id(0x822)
    );
}

#[test]
fn hst_23_winning_head_models_and_extra_parts_resolve_override_namespaces() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(T(b"NPC_"), 0x830).sub(T(b"PNAM"), 0x822u32.to_le_bytes()));
    base.push_top(head(0x822, 1, "original.nif"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".into()],
        ..Default::default()
    });
    patch.push_top(head(0x822, 1, "override.nif").sub(T(b"HNAM"), 0x01000823u32.to_le_bytes()));
    patch.push_top(head(0x01000823, 0, "extra.nif"));
    let mut order = LoadOrder::new();
    order.add("Base.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let view =
        ActorAppearance::from_order(&order, Some(id(0x830)), None, &BTreeSet::new()).unwrap();
    assert_eq!(view.head_parts[0].model.as_deref(), Some("override.nif"));
    let face = view.facegen.as_ref().unwrap();
    assert_eq!((face.plugin.as_str(), face.object), ("Base.esm", 0x830));
    assert_eq!(
        view.head_parts[1].form,
        GlobalFormId {
            file: 1,
            object: 0x823
        }
    );
}

#[test]
fn hst_23_trait_templates_supply_their_heads_and_sex() {
    let mut acbs = vec![0; 24];
    acbs[18..20].copy_from_slice(&npc::template::TRAITS.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), 0x830)
            .sub(T(b"ACBS"), acbs)
            .sub(T(b"TPLT"), 0x831u32.to_le_bytes()),
    );
    let mut acbs = vec![0; 24];
    acbs[0] = 1;
    plugin.push_top(
        Record::new(T(b"NPC_"), 0x831)
            .sub(T(b"ACBS"), acbs)
            .sub(T(b"PNAM"), 0x822u32.to_le_bytes()),
    );
    plugin.push_top(head(0x822, 1, "template_head.nif"));
    let mut order = LoadOrder::new();
    order.add("Template.esm", plugin).unwrap();
    let view =
        ActorAppearance::from_order(&order, Some(id(0x830)), None, &BTreeSet::new()).unwrap();
    assert!(view.female);
    assert_eq!(view.facegen.as_ref().unwrap().object, 0x831);
    assert_eq!(
        view.head_parts[0].model.as_deref(),
        Some("template_head.nif")
    );
}

#[test]
fn hst_23_initial_disable_and_chained_opposite_enable_parents_filter_contacts() {
    let mut game = fixture();
    game.change_cell(Some(id(0x840)));
    let actors = |game: &mut Game| {
        game.world_mut()
            .contact_actors()
            .into_iter()
            .map(|actor| actor.actor)
            .collect::<Vec<_>>()
    };
    assert_eq!(actors(&mut game), [id(0x850), id(0x854)]);
    game.world_mut().set_reference_enabled(id(0x852), true);
    assert_eq!(actors(&mut game), [id(0x850), id(0x852), id(0x853)]);
    game.world_mut().set_reference_enabled(id(0x852), false);
    assert_eq!(actors(&mut game), [id(0x850), id(0x854)]);
}
