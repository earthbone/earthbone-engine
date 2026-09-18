// SPDX-License-Identifier: GPL-3.0-or-later

//! Gated on `SKYRIM_DATA`, these drive the viseme pipeline over the installed
//! game: the races whose mapping the tracks are played through, real voice
//! lines, and the case vanilla cannot do at all, a line with no LIP block.
//! They skip cleanly when the data, the features or the tools are absent, and
//! they never copy game data anywhere.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::records::race::Race;
use soe_esm::{tag, LoadOrder, Plugin};

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

fn voice_archive() -> Option<soe_bsa::Archive> {
    let dir = data_dir()?;
    let path = dir.join("Skyrim - Voices_en0.bsa");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    Some(soe_bsa::Archive::open(&path).expect("the voice archive opens"))
}

/// Just Skyrim.esm: every race the base game defines is in it, and the DLC
/// add no phoneme mapping this test needs.
fn base_order() -> Option<LoadOrder> {
    let dir = data_dir()?;
    let path = dir.join("Skyrim.esm");
    if !path.exists() {
        eprintln!("skipping: {} is not there", path.display());
        return None;
    }
    let bytes = std::fs::read(&path).expect("Skyrim.esm reads");
    let plugin = Plugin::parse(&bytes).expect("Skyrim.esm parses");
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).expect("it loads");
    Some(order)
}

fn race_named(order: &LoadOrder, editor_id: &str) -> Option<Race> {
    order
        .winners_of(tag::RACE)
        .find(|(_, r)| r.editor_id().as_deref() == Some(editor_id))
        .map(|(_, r)| Race::from_record(r).expect("the race reads"))
}

#[test]
fn the_vanilla_races_carry_the_phoneme_mapping_a_face_is_driven_from() {
    let Some(order) = base_order() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let mut total = 0;
    let mut complete = Vec::new();
    for (_, rec) in order.winners_of(tag::RACE) {
        let race = Race::from_record(rec).expect("a race reads");
        total += 1;
        if race.phoneme_mapping_is_complete() {
            complete.push((
                race.editor_id.clone().unwrap_or_default(),
                race.phoneme_targets.clone(),
            ));
        }
    }
    eprintln!(
        "races: {total}, with a complete phoneme mapping: {}",
        complete.len()
    );
    for (name, targets) in &complete {
        eprintln!("  {name}: {} targets {targets:?}", targets.len());
    }

    // The finding this test exists to pin: the mapping is sparse. Vanilla
    // writes it on a handful of races and leaves the rest empty, so the
    // consumer needs a fallback race rather than assuming the actor's own.
    assert!(!complete.is_empty(), "no vanilla race carried a mapping");
    assert!(
        complete.len() < total / 2,
        "the mapping stopped being sparse, the fallback may no longer be needed"
    );
    let nord = race_named(&order, "NordRace").expect("NordRace exists");
    assert!(
        nord.phoneme_targets.is_empty(),
        "NordRace gained a mapping, so the fallback assumption needs revisiting"
    );
}

/// The `rhubarb` binary, from `SOE_RHUBARB`, then the copy under `tools/`,
/// then the path.
#[cfg(feature = "rhubarb")]
fn rhubarb() -> Option<soe_audio::rhubarb::Rhubarb> {
    use soe_audio::rhubarb::Rhubarb;

    if let Some(p) = std::env::var_os("SOE_RHUBARB") {
        let r = Rhubarb::at(p.to_string_lossy().into_owned());
        if r.available() {
            return Some(r);
        }
    }
    let tools = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tools/rhubarb")
        .canonicalize()
        .ok();
    if let Some(tools) = tools {
        if let Ok(entries) = std::fs::read_dir(&tools) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("rhubarb");
                if candidate.is_file() {
                    let r = Rhubarb::at(candidate.to_string_lossy().into_owned());
                    if r.available() {
                        return Some(r);
                    }
                }
            }
        }
    }
    let r = Rhubarb::new();
    if r.available() {
        return Some(r);
    }
    eprintln!("skipping: no rhubarb binary found");
    None
}

#[cfg(all(feature = "rhubarb", feature = "xwma-ffmpeg"))]
#[test]
fn real_voice_lines_turn_into_mouth_shapes() {
    use soe_audio::viseme::{Shape, VisemeSource};
    use soe_audio::wma::Ffmpeg;
    use soe_audio::{Fuz, Xwma};

    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: the ffmpeg binary is not on the path");
        return;
    }
    let Some(source) = rhubarb() else { return };

    let names: Vec<String> = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".fuz"))
        .take(8)
        .cloned()
        .collect();
    assert!(!names.is_empty(), "the voice archive held no FUZ files");

    let mut shapes = std::collections::BTreeMap::<char, usize>::new();
    let mut produced = 0;
    let mut audio_seconds = 0.0f64;
    let started = std::time::Instant::now();
    for name in &names {
        let bytes = archive.read(name).expect("a voice file reads");
        let fuz = Fuz::split(&bytes).expect("it splits");
        let xwma = Xwma::parse(fuz.audio).expect("the container parses");
        let pcm = soe_audio::wma::decode_with(&decoder, &xwma).expect("it decodes");
        audio_seconds += pcm.duration_seconds();
        let track = source
            .visemes(&pcm, None)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!track.cues.is_empty(), "{name} produced no cues");
        // A track has to cover the line, not just its first moment.
        assert!(
            track.duration > 0.0,
            "{name} produced a track with no duration"
        );
        for cue in &track.cues {
            *shapes.entry(cue.shape.letter()).or_default() += 1;
        }
        produced += 1;
    }
    let elapsed = started.elapsed().as_secs_f64();
    eprintln!(
        "viseme tracks: {produced} of {} lines, {audio_seconds:.2}s of audio, {elapsed:.2}s wall, {:.2}s per line, {:.2}x realtime",
        names.len(),
        elapsed / produced as f64,
        audio_seconds / elapsed
    );
    eprintln!("shape distribution:");
    let total: usize = shapes.values().sum();
    for (letter, count) in &shapes {
        eprintln!(
            "  {letter}: {count} cues, {:.1}%",
            *count as f64 / total as f64 * 100.0
        );
    }
    assert_eq!(produced, names.len());
    // Speech that only ever rests would mean the recogniser heard nothing.
    let talking: usize = shapes
        .iter()
        .filter(|(l, _)| **l != Shape::X.letter())
        .map(|(_, c)| *c)
        .sum();
    assert!(talking > 0, "every cue was the rest position");
}

/// The case the vanilla pipeline cannot do at all.
///
/// A mod that adds a voice line ships audio with no LIP block, because
/// generating one means the Creation Kit and its `FonixData.cdf`. Here a real
/// line is re-encoded to Ogg Vorbis, which drops the FUZ container and the
/// LIP block with it, decoded through the pure Rust decoder, and run through
/// the same pipeline a vanilla line uses. Then the track is played through a
/// real race's mapping, so this covers the whole chain.
#[cfg(all(feature = "rhubarb", feature = "xwma-ffmpeg", feature = "vorbis"))]
#[test]
fn a_mod_line_with_no_lip_block_animates_like_a_vanilla_one() {
    use soe_audio::viseme::{morph_weights, VisemeSource};
    use soe_audio::wma::Ffmpeg;
    use soe_audio::{Container, Fuz, Xwma};
    use std::process::{Command, Stdio};

    let Some(archive) = voice_archive() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let decoder = Ffmpeg::new();
    if !decoder.available() {
        eprintln!("skipping: the ffmpeg binary is not on the path");
        return;
    }
    let Some(source) = rhubarb() else { return };
    let Some(order) = base_order() else { return };
    // NordRace carries no mapping, which is the normal case, so the face is
    // driven through a donor that does. See `mapping_race`.
    let nord = race_named(&order, "NordRace").expect("NordRace exists");
    let donor = race_named(&order, "WerewolfBeastRace").expect("WerewolfBeastRace exists");
    let face = soe_audio::mapping_race(&nord, &donor);
    assert!(
        face.phoneme_mapping_is_complete(),
        "the donor race has to carry a mapping"
    );

    // Not every FUZ carries lip data, so pick one that does. The contrast is
    // the point: this line has a LIP block and the mod version will not.
    let (name, bytes, fuz) = archive
        .files()
        .iter()
        .filter(|n| n.to_lowercase().ends_with(".fuz"))
        .take(64)
        .find_map(|n| {
            let bytes = archive.read(n).ok()?;
            let fuz = Fuz::split(&bytes).ok()?;
            if fuz.lip.is_empty() {
                return None;
            }
            let audio = fuz.audio.to_vec();
            Some((n.clone(), bytes.len(), audio))
        })
        .expect("a voice line with a LIP block");
    let xwma = Xwma::parse(&fuz).expect("the container parses");
    eprintln!("vanilla line {name}: {bytes} bytes of FUZ, with a LIP block");

    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "xwma",
            "-i",
            "pipe:0",
            "-c:a",
            "libvorbis",
            "-f",
            "ogg",
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ffmpeg runs");
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin");
        let _ = stdin.write_all(&xwma.to_bytes());
    }
    let out = child.wait_with_output().expect("ffmpeg finishes");
    if !out.status.success() {
        eprintln!(
            "skipping: this ffmpeg build cannot encode Vorbis: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return;
    }

    // What a mod ships: an Ogg file, nothing else. No FUZ, so no LIP.
    let mod_file = out.stdout;
    assert_eq!(soe_audio::sniff(&mod_file), Container::OggVorbis);
    assert!(
        Fuz::split(&mod_file).is_err(),
        "a bare Ogg file is not a FUZ and must not pretend to be"
    );

    let pcm = soe_audio::vorbis::decode(&mod_file).expect("the mod's audio decodes");
    let track = source.visemes(&pcm, None).expect("visemes from a mod line");
    assert!(
        !track.cues.is_empty(),
        "a mod line with no LIP block produced no mouth shapes"
    );

    // And the track drives a real face: weights land on the race's own
    // targets and at least one is non-zero somewhere in the line.
    let mut moved = false;
    let mut steps = 0;
    let mut t = 0.0;
    while t < track.duration {
        let weights = morph_weights(&track.shapes_at(t, source.blend), face);
        assert_eq!(weights.len(), face.phoneme_targets.len());
        if weights.iter().any(|(_, w)| *w != 0.0) {
            moved = true;
        }
        steps += 1;
        t += 1.0 / 60.0;
    }
    eprintln!(
        "mod line from {name}: {} bytes of Ogg, {:.2}s, {} cues, {steps} frames sampled on {:?}",
        mod_file.len(),
        track.duration,
        track.cues.len(),
        face.editor_id
    );
    assert!(moved, "the face never moved across the whole line");
}
