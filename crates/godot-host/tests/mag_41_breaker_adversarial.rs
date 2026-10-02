// SPDX-License-Identifier: GPL-3.0-or-later

const HOST_CHECK: &str = include_str!("../../../godot/mag_41_telekinesis_host_test.gd");

#[test]
fn mag_41_interruption_check_uses_the_live_player_and_streamer() {
    assert!(HOST_CHECK.contains("ClassDB.instantiate(\"PlayerNode\")"));
    assert!(HOST_CHECK.contains("player.call(\"use_streamer_data\", streamer)"));
    assert!(HOST_CHECK.contains("player.call(\"enter_cell\", CELL"));
    assert!(HOST_CHECK.contains("player.call(\"input_focus_lost\")"));
    let player_created = HOST_CHECK
        .find("ClassDB.instantiate(\"PlayerNode\")")
        .unwrap_or_default();
    let streamer_connected = HOST_CHECK
        .find("player.call(\"use_streamer_data\", streamer)")
        .unwrap_or_default();
    let cell_entered = HOST_CHECK
        .find("player.call(\"enter_cell\", CELL")
        .unwrap_or_default();
    let focus_lost = HOST_CHECK
        .find("player.call(\"input_focus_lost\")")
        .unwrap_or_default();

    assert!(player_created < streamer_connected);
    assert!(streamer_connected < cell_entered);
    assert!(cell_entered < focus_lost);
}

#[test]
fn mag_41_interruption_check_does_not_release_the_body_manually() {
    assert!(HOST_CHECK.contains("player.call(\"input_focus_lost\")"));
    let focus_lost = HOST_CHECK
        .find("player.call(\"input_focus_lost\")")
        .unwrap_or_default();
    let manual_release =
        HOST_CHECK[focus_lost..].find("streamer.call(\"telekinesis_release_reference\"");

    assert!(
        manual_release.is_none(),
        "a direct streamer release can pass even when PlayerNode cleanup did nothing"
    );
}
