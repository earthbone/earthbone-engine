// SPDX-License-Identifier: GPL-3.0-or-later

//! Gated on `SKYRIM_DATA`: a real footstep and a real voice line travel the
//! whole host path, from the archive to the exact bytes `AudioStreamWav`
//! would be handed.
//!
//! What this can and cannot prove. It runs the chain the shipped code runs,
//! `SoundSource` to [`Pcm`] to 16 bit mono bytes, and asserts the result
//! carries signal rather than silence, which is the failure a conversion bug
//! actually produces. It stops one step short of a speaker, because the last
//! step belongs to `AudioStreamPlayer3D` and needs a Godot process and an
//! audio device. `docs/godot-host.md` carries the windowed check for that.
//!
//! Paths are discovered rather than hardcoded, so this keeps working on an
//! install with different DLC.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_audio::SoundSource;
use soe_bsa::Vfs;
use soe_godot_host::audio::{to_i16_bytes, to_mono};
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    std::env::var_os("SKYRIM_DATA").map(PathBuf::from)
}

/// A VFS over the two sound archives, the same pair `SoundBank::open_data`
/// indexes.
fn sound_vfs() -> Option<Vfs> {
    let dir = data_dir()?;
    let mut vfs = Vfs::new();
    for name in ["Skyrim - Sounds.bsa", "Skyrim - Voices_en0.bsa"] {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("skipping: {} is not there", path.display());
            return None;
        }
        vfs.add_archive_path(&path).expect("the archive opens");
    }
    Some(vfs)
}

/// Extension match that ignores case, since an archive may spell one either
/// way and a case-sensitive compare would silently miss half a folder.
fn has_ext(name: &str, ext: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// The first file in the VFS whose path contains every fragment given.
fn find_one(vfs: &Vfs, fragments: &[&str]) -> Option<String> {
    let mut hits: Vec<&str> = vfs
        .paths()
        .filter(|n| fragments.iter().all(|f| n.contains(f)))
        .collect();
    // Sorted so a run picks the same file twice and a failure is reproducible.
    hits.sort_unstable();
    hits.first().map(|s| (*s).to_string())
}

#[test]
fn a_real_footstep_becomes_bytes_godot_can_play() {
    let Some(vfs) = sound_vfs() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source = SoundSource::new(&vfs);

    let path = find_one(&vfs, &["sound/fx/fst/", ".wav"]).expect("the archive ships footsteps");
    let sound = source.read(&path).expect("a footstep decodes");
    let mono = to_mono(&sound.pcm);
    let bytes = to_i16_bytes(&mono);

    eprintln!("footstep {path}");
    eprintln!(
        "  {} Hz, {} channels, {} frames, {:.3} s, peak {:.4}",
        sound.pcm.sample_rate,
        sound.pcm.channels,
        sound.pcm.frames(),
        sound.pcm.duration_seconds(),
        sound.pcm.peak()
    );
    eprintln!("  {} bytes to AudioStreamWav", bytes.len());

    // Two bytes per mono frame is exactly what FORMAT_16_BITS expects, and a
    // mismatch here is the bug that makes a sound play at the wrong speed.
    assert_eq!(bytes.len(), mono.frames() * 2);
    assert_eq!(mono.channels, 1);
    assert!(mono.frames() > 0, "a footstep decoded to nothing");
    // Silence converts to silence without erroring, so peak is the assertion
    // that separates working audio from a buffer of zeroes.
    assert!(
        sound.pcm.peak() > 0.01,
        "the footstep decoded to near silence, peak {}",
        sound.pcm.peak()
    );
    assert!(
        bytes.iter().any(|&b| b != 0),
        "every converted byte was zero"
    );
}

#[test]
#[cfg(feature = "xwma-ffmpeg")]
fn a_real_voice_line_becomes_bytes_godot_can_play() {
    use soe_audio::wma::Ffmpeg;

    let Some(vfs) = sound_vfs() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let wma = Ffmpeg::new();
    if !wma.available() {
        eprintln!("skipping: ffmpeg is not on PATH");
        return;
    }
    let source = SoundSource::new(&vfs).with_wma_decoder(&wma);

    // Surveyed rather than taking the first path alphabetically. That one is
    // a `crdogvoice` line with peak 0.0015, and creature dialogue entries are
    // routinely silent placeholders, so picking alphabetically measured
    // Bethesda's content rather than this crate's conversion.
    let mut names: Vec<&str> = vfs
        .paths()
        .filter(|n| n.contains("sound/voice/skyrim.esm/") && has_ext(n, "fuz"))
        .collect();
    names.sort_unstable();
    let stride = (names.len() / 60).max(1);
    let sampled: Vec<&str> = names.iter().step_by(stride).take(60).copied().collect();

    let (mut signal, mut silent) = (0, 0);
    let mut loudest: Option<(f32, String)> = None;
    for name in &sampled {
        let Ok(s) = source.read(name) else { continue };
        let peak = s.pcm.peak();
        if peak > 0.01 {
            signal += 1;
        } else {
            silent += 1;
        }
        if loudest.as_ref().is_none_or(|(p, _)| peak > *p) {
            loudest = Some((peak, (*name).to_string()));
        }
    }
    eprintln!("voice survey over {} sampled lines:", sampled.len());
    eprintln!("  carried signal {signal}");
    eprintln!("  near silent    {silent}");
    assert!(
        signal > silent,
        "most sampled voice lines decoded to silence, which points at the decoder rather than the data"
    );

    let path = loudest.expect("at least one voice line decoded").1;
    let sound = source.read(&path).expect("a voice line decodes");
    let mono = to_mono(&sound.pcm);
    let bytes = to_i16_bytes(&mono);

    eprintln!("voice {path}");
    eprintln!(
        "  {} Hz, {} channels, {} frames, {:.3} s, peak {:.4}, lip {}",
        sound.pcm.sample_rate,
        sound.pcm.channels,
        sound.pcm.frames(),
        sound.pcm.duration_seconds(),
        sound.pcm.peak(),
        sound.lip.as_ref().map_or(0, Vec::len)
    );
    eprintln!("  {} bytes to AudioStreamWav", bytes.len());

    assert_eq!(bytes.len(), mono.frames() * 2);
    assert_eq!(mono.channels, 1);
    assert!(mono.frames() > 0, "a voice line decoded to nothing");
    assert!(
        sound.pcm.peak() > 0.01,
        "the voice line decoded to near silence, peak {}",
        sound.pcm.peak()
    );
}

#[test]
fn the_conversion_holds_across_many_real_footsteps() {
    let Some(vfs) = sound_vfs() else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source = SoundSource::new(&vfs);

    let mut names: Vec<&str> = vfs
        .paths()
        .filter(|n| n.contains("sound/fx/fst/") && has_ext(n, "wav"))
        .collect();
    names.sort_unstable();
    names.truncate(200);
    if names.is_empty() {
        eprintln!("skipping: the archive lists no footsteps");
        return;
    }

    let (mut ok, mut silent, mut failed) = (0, 0, 0);
    let mut rates = std::collections::BTreeSet::new();
    for name in &names {
        let Ok(sound) = source.read(name) else {
            failed += 1;
            continue;
        };
        rates.insert(sound.pcm.sample_rate);
        let mono = to_mono(&sound.pcm);
        let bytes = to_i16_bytes(&mono);
        assert_eq!(bytes.len(), mono.frames() * 2, "{name} converted short");
        if sound.pcm.peak() > 0.01 {
            ok += 1;
        } else {
            silent += 1;
        }
    }

    eprintln!("footstep conversion over {} files:", names.len());
    eprintln!("  carried signal {ok}");
    eprintln!("  near silent    {silent}");
    eprintln!("  failed to read {failed}");
    eprintln!("  sample rates   {rates:?}");

    assert_eq!(failed, 0, "some footsteps did not decode");
    assert!(ok > 0, "no footstep carried any signal");
}
