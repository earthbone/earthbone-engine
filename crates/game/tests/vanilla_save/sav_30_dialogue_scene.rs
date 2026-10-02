// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use soe_save::ChangeForm;

const INFO: u32 = 0x800;
const SMQN: u32 = 0x801;
const SCEN_ACTIVE: u32 = 0x802;
const SCEN_INACTIVE: u32 = 0x803;

fn dialogue_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for (tag, id) in [
        (b"INFO", INFO),
        (b"SMQN", SMQN),
        (b"SCEN", SCEN_ACTIVE),
        (b"SCEN", SCEN_INACTIVE),
    ] {
        let mut record = Record::new(Tag(*tag), id);
        record.flags = match id {
            INFO => 0x0000_F00F,
            SMQN => 0x000F_0F0F,
            SCEN_ACTIVE => 0xFFFF_00DF,
            SCEN_INACTIVE => 0x0000_1010,
            _ => unreachable!(),
        };
        if tag == b"SCEN" {
            record = record
                .sub(Tag(*b"HNAM"), [])
                .zsub(Tag(*b"NAM0"), "First")
                .sub(Tag(*b"HNAM"), [])
                .sub(Tag(*b"ANAM"), []);
        }
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    Game::new(order)
}

fn change(form_index: u32, change_flags: u32, type_field: u8, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: reference(form_index),
        change_flags,
        type_field,
        version: 0,
        uncompressed_len: 0,
        data,
    }
}

fn fixture() -> Save {
    let mut save = save();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.light_plugins.clear();
    save.form_id_array = vec![INFO, SMQN, SCEN_ACTIVE, SCEN_INACTIVE];

    let mut info = Vec::new();
    info.extend(0xA5_u32.to_le_bytes());
    info.extend(0x7B_u16.to_le_bytes());
    let mut smqn = Vec::new();
    smqn.extend(0x5A_u32.to_le_bytes());
    smqn.extend(0xC3_u16.to_le_bytes());
    smqn.extend(2_u32.to_le_bytes());
    smqn.extend(0x7FC0_1234_u32.to_le_bytes());
    smqn.extend((-0.0_f32).to_bits().to_le_bytes());
    let mut scene = Vec::new();
    scene.extend(0x1214_u32.to_le_bytes());
    scene.extend(0x5678_u16.to_le_bytes());
    scene.extend([0x10, 0x20, 0x30]);
    let mut inactive_scene = Vec::new();
    inactive_scene.extend(0x4321_u32.to_le_bytes());
    inactive_scene.extend(0x8765_u16.to_le_bytes());

    save.change_forms = vec![
        change(1, 1 | (1 << 31), 7, info),
        change(2, 1 | (1 << 31), 36, smqn),
        change(3, 1 | (1 << 31), 37, scene),
        change(4, 1, 37, inactive_scene),
    ];
    save
}

#[test]
fn sav_30_info_smqn_and_scen_changes_reach_independent_live_queries() {
    let game = dialogue_game();
    assert_eq!(
        game.world().record(id(0, SCEN_ACTIVE)).unwrap().tag,
        Tag(*b"SCEN")
    );
    let report = game
        .world_mut()
        .restore_vanilla_dialogue_scene_changes(&fixture())
        .unwrap();
    assert_eq!(report.infos, 1);
    assert_eq!(report.story_manager_quest_nodes, 1);
    assert_eq!(report.scenes, 2);

    let world = game.world();
    assert_eq!(world.restored_info_said_once(id(0, INFO)), Some(true));
    assert_eq!(world.record(id(0, INFO)).unwrap().flags, 0x0000_F0AA);
    assert_eq!(
        world.restored_dialogue_form_flags(id(0, INFO)),
        Some((0xA5, 0x7B))
    );
    assert_eq!(
        world.restored_smqn_last_run_times(id(0, SMQN)),
        Some(&[0x7FC0_1234, (-0.0_f32).to_bits()][..])
    );
    assert_eq!(world.record(id(0, SMQN)).unwrap().flags, 0x000F_0F55);
    assert!(world.scenes.contains(&id(0, SCEN_ACTIVE)));
    assert!(!world.scenes.contains(&id(0, SCEN_INACTIVE)));
    assert_eq!(
        world.restored_scene_active_data(id(0, SCEN_ACTIVE)),
        Some(&[0x10, 0x20, 0x30][..])
    );
    assert_eq!(world.record(id(0, SCEN_ACTIVE)).unwrap().flags, 0xFFFF_12CB);
    assert_eq!(
        world.record(id(0, SCEN_INACTIVE)).unwrap().flags,
        0x0000_5331
    );
    assert_eq!(world.scene_phase(id(0, SCEN_ACTIVE)), Some(0));
    assert_eq!(world.scene_phase(id(0, SCEN_INACTIVE)), None);
    assert_eq!(
        world.restored_dialogue_form_flags(id(0, SCEN_ACTIVE)),
        Some((0x1214, 0x5678))
    );
    assert_eq!(
        world.restored_dialogue_form_flags(id(0, SCEN_INACTIVE)),
        Some((0x4321, 0x8765))
    );
}

#[test]
fn sav_30_restored_record_changes_survive_engine_save_load() {
    let game = dialogue_game();
    game.world_mut()
        .restore_vanilla_dialogue_scene_changes(&fixture())
        .unwrap();
    let bytes = game.save_state().unwrap();
    let mut restored = dialogue_game();
    restored.load_state(&bytes).unwrap();
    let world = restored.world();

    assert_eq!(world.restored_info_said_once(id(0, INFO)), Some(true));
    assert_eq!(world.record(id(0, INFO)).unwrap().flags, 0x0000_F0AA);
    assert_eq!(
        world.restored_dialogue_form_flags(id(0, SMQN)),
        Some((0x5A, 0xC3))
    );
    assert_eq!(world.record(id(0, SMQN)).unwrap().flags, 0x000F_0F55);
    assert_eq!(
        world.restored_smqn_last_run_times(id(0, SMQN)),
        Some(&[0x7FC0_1234, (-0.0_f32).to_bits()][..])
    );
    assert_eq!(
        world.restored_scene_active_data(id(0, SCEN_ACTIVE)),
        Some(&[0x10, 0x20, 0x30][..])
    );
    assert_eq!(
        world.restored_dialogue_form_flags(id(0, SCEN_ACTIVE)),
        Some((0x1214, 0x5678))
    );
    assert!(world.scenes.contains(&id(0, SCEN_ACTIVE)));
    assert!(!world.scenes.contains(&id(0, SCEN_INACTIVE)));
    assert_eq!(world.scene_phase(id(0, SCEN_ACTIVE)), Some(0));
    assert_eq!(world.scene_phase(id(0, SCEN_INACTIVE)), None);
}

#[test]
fn sav_30_invalid_last_change_leaves_all_world_state_unchanged() {
    let game = dialogue_game();
    let mut saved = fixture();
    saved.change_forms.push(change(1, 0, 7, vec![]));
    let before = game.save_state().unwrap();
    assert!(game
        .world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .is_err());
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn sav_30_installed_dialogue_scene_records_apply_form_flag_masks() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let plugin =
        Plugin::parse(&std::fs::read(Path::new(&data).join("Skyrim.esm")).unwrap()).unwrap();
    let selected: Vec<_> = [*b"INFO", *b"SMQN", *b"SCEN"]
        .into_iter()
        .map(|tag| {
            plugin
                .records
                .iter()
                .find(|record| record.tag == Tag(tag))
                .unwrap_or_else(|| {
                    panic!("Skyrim.esm is missing {}", String::from_utf8_lossy(&tag))
                })
                .form_id
        })
        .collect();
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let expected: Vec<_> = selected
        .iter()
        .map(|form| order.winner(id(0, *form)).unwrap().flags ^ 1)
        .collect();
    let mut saved = fixture();
    saved.form_id_array.clone_from(&selected);
    saved.change_forms = [7, 36, 37]
        .into_iter()
        .enumerate()
        .map(|(index, type_field)| {
            let mut payload = 1_u32.to_le_bytes().to_vec();
            payload.extend(0_u16.to_le_bytes());
            change(index as u32 + 1, 1, type_field, payload)
        })
        .collect();
    let mut world = World::new(order);
    let report = world
        .restore_vanilla_dialogue_scene_changes(&saved)
        .unwrap();
    assert_eq!(report.infos, 1);
    assert_eq!(report.story_manager_quest_nodes, 1);
    assert_eq!(report.scenes, 1);
    for (form, expected) in selected.into_iter().zip(expected) {
        assert_eq!(world.record(id(0, form)).unwrap().flags, expected);
    }
}
