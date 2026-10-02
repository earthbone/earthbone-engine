// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn sav_30_adversarial_zero_form_flag_masks_preserve_live_flags_after_save_load() {
    let game = dialogue_game();
    let mut saved = fixture();
    for change in &mut saved.change_forms {
        if change.change_flags & 1 != 0 {
            change.data[..4].copy_from_slice(&0_u32.to_le_bytes());
        }
    }

    game.world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .unwrap();
    assert_eq!(game.world().record(id(0, INFO)).unwrap().flags, 0x0000_F00F);
    assert_eq!(game.world().record(id(0, SMQN)).unwrap().flags, 0x000F_0F0F);
    assert_eq!(
        game.world().record(id(0, SCEN_ACTIVE)).unwrap().flags,
        0xFFFF_00DF
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = dialogue_game();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.world().record(id(0, INFO)).unwrap().flags, 0x0000_F00F);
    assert_eq!(loaded.world().record(id(0, SMQN)).unwrap().flags, 0x000F_0F0F);
    assert_eq!(
        loaded.world().record(id(0, SCEN_ACTIVE)).unwrap().flags,
        0xFFFF_00DF
    );
}

#[test]
fn sav_30_adversarial_wrong_type_after_valid_changes_leaves_world_unchanged() {
    let game = dialogue_game();
    let mut saved = fixture();
    let mut payload = 0_u32.to_le_bytes().to_vec();
    payload.extend(0_u16.to_le_bytes());
    saved.change_forms.push(change(4, 1, 7, payload));
    let before = game.save_state().unwrap();

    assert!(game
        .world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .is_err());
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn sav_30_adversarial_inactive_scene_rejects_active_payload_bytes_atomically() {
    let game = dialogue_game();
    let mut saved = fixture();
    saved.change_forms[3].data.push(0xA5);
    let before = game.save_state().unwrap();

    assert!(game
        .world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .is_err());
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn sav_30_adversarial_repeated_restore_rejects_conflicting_runtime_override_atomically() {
    let game = dialogue_game();
    let saved = fixture();
    game.world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .unwrap();
    let before = game.save_state().unwrap();

    assert!(game
        .world_mut()
        .restore_vanilla_dialogue_scene_changes(&saved)
        .is_err());
    assert_eq!(game.save_state().unwrap(), before);
}
