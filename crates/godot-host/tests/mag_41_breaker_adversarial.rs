// SPDX-License-Identifier: GPL-3.0-or-later

const HOST_CHECK: &str = include_str!("../../../godot/mag_41_telekinesis_host_test.gd");

#[test]
fn mag_41_interruption_check_uses_the_live_player_and_streamer() {
    let player_created = HOST_CHECK
        .find("ClassDB.instantiate(\"PlayerNode\")")
        .expect("the host check must create a PlayerNode");
    let streamer_connected = HOST_CHECK
        .find("player.call(\"use_streamer_data\", streamer)")
        .expect("the tested player must share the streamer holding the body");
    let cell_entered = HOST_CHECK
        .find("player.call(\"enter_cell\", CELL")
        .expect("the tested player must enter the resident test cell");
    let focus_lost = HOST_CHECK
        .find("player.call(\"input_focus_lost\")")
        .expect("the interruption must reach PlayerNode focus cleanup");

    assert!(player_created < streamer_connected);
    assert!(streamer_connected < cell_entered);
    assert!(cell_entered < focus_lost);
}

#[test]
fn mag_41_interruption_check_does_not_release_the_body_manually() {
    let focus_lost = HOST_CHECK
        .find("player.call(\"input_focus_lost\")")
        .expect("the interruption must reach PlayerNode focus cleanup");
    let manual_release = HOST_CHECK[focus_lost..]
        .find("streamer.call(\"telekinesis_release_reference\"");

    assert!(
        manual_release.is_none(),
        "a direct streamer release can pass even when PlayerNode cleanup did nothing"
    );
}
