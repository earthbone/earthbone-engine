// SPDX-License-Identifier: GPL-3.0-or-later

use soe_godot_host::audio::MusicFade;

#[test]
fn aud_05_music_fade_applies_both_record_durations_and_finishes_cleanly() {
    let mut fade = MusicFade::new(0.8, 2.0, 4.0, false);
    assert_eq!(fade.advance(1.0), (0.6, 0.5, false));
    assert_eq!(fade.advance(1.0), (0.4, 1.0, false));
    assert_eq!(fade.advance(2.0), (0.0, 1.0, true));
}

#[test]
fn aud_05_music_abrupt_transition_ends_both_slots_without_a_tail() {
    let mut fade = MusicFade::new(1.0, 3.0, 8.0, true);
    assert_eq!(fade.advance(0.0), (0.0, 1.0, true));
}

#[test]
fn aud_05_music_restarted_transition_does_not_retain_an_old_fade_object() {
    let mut first = MusicFade::new(1.0, 5.0, 5.0, false);
    let (old_level, new_level, done) = first.advance(1.0);
    assert_eq!((old_level, new_level, done), (0.8, 0.2, false));

    let mut latest = MusicFade::new(new_level, 1.0, 1.0, false);
    assert_eq!(latest.advance(1.0), (0.0, 1.0, true));
}
