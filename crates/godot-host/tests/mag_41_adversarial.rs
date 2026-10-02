// SPDX-License-Identifier: GPL-3.0-or-later

const HOST_CHECK: &str = include_str!("../../../godot/mag_41_telekinesis_host_test.gd");

#[test]
fn mag_41_interruption_check_connects_player_to_the_streamer() {
    assert!(
        HOST_CHECK.contains("player.call(\"use_streamer_data\", streamer)"),
        "the player must share the streamer that owns the held physics body"
    );
    assert!(
        HOST_CHECK.contains("player.call(\"enter_cell\", CELL"),
        "the player must enter the tested cell before it handles interruption"
    );
}

#[test]
fn mag_41_interruption_check_uses_player_cleanup_for_host_release() {
    assert!(HOST_CHECK.contains("player.call(\"input_focus_lost\")"));
    assert!(
        !HOST_CHECK.contains("streamer.call(\"telekinesis_release_reference\""),
        "manual streamer release bypasses the PlayerNode interruption path"
    );
}
