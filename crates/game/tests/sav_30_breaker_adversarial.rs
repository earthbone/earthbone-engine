// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn sav_30_breaker_saved_scene_phase_reaches_runtime_and_survives_engine_save_load() {
    let game = dialogue_game();
    let mut saved = fixture();
    saved.change_forms[2].data[7..11].copy_from_slice(&1_u32.to_le_bytes());

    game.world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .unwrap();
    assert_eq!(game.world().scene_phase(id(0, SCEN_ACTIVE)), Some(1));

    let bytes = game.save_state().unwrap();
    let mut loaded = dialogue_game();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.world().scene_phase(id(0, SCEN_ACTIVE)), Some(1));
}

#[test]
fn sav_30_breaker_state_without_form_flags_preserves_record_flags_after_save_load() {
    let game = dialogue_game();
    let mut saved = fixture();
    saved.change_forms = vec![
        change(1, 1 << 31, 7, vec![]),
        change(2, 1 << 31, 36, 0_u32.to_le_bytes().to_vec()),
        change(3, 1 << 31, 37, scene_runtime()),
    ];

    game.world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .unwrap();
    assert_eq!(game.world().record(id(0, INFO)).unwrap().flags, 0x0000_F00F);
    assert_eq!(game.world().record(id(0, SMQN)).unwrap().flags, 0x000F_0F0F);
    assert_eq!(
        game.world().record(id(0, SCEN_ACTIVE)).unwrap().flags,
        0xFFFF_00DF
    );
    assert_eq!(
        game.world().restored_smqn_last_run_times(id(0, SMQN)),
        Some(&[][..])
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = dialogue_game();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(
        loaded.world().record(id(0, INFO)).unwrap().flags,
        0x0000_F00F
    );
    assert_eq!(
        loaded.world().record(id(0, SMQN)).unwrap().flags,
        0x000F_0F0F
    );
    assert_eq!(
        loaded.world().record(id(0, SCEN_ACTIVE)).unwrap().flags,
        0xFFFF_00DF
    );
    assert_eq!(
        loaded.world().restored_info_said_once(id(0, INFO)),
        Some(true)
    );
}
