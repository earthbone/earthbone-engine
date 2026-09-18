// SPDX-License-Identifier: GPL-3.0-or-later

//! A node that plays vanilla sounds through Godot's own audio path.
//!
//! Godot ships a mixer, a device backend and 3D attenuation, and the user's
//! ruling on physics was that where the engine already has a subsystem we use
//! it rather than linking our own. The plan row for this item offered `cpal`
//! or `rodio`, and both would put a second audio stack in the tree beside the
//! one gdext already links, so neither is taken. What is here is the seam:
//! `soe_audio` decodes to [`Pcm`], [`crate::audio`] converts, and
//! `AudioStreamPlayer3D` does the rest.
//!
//! Why `AudioStreamWav` and not `AudioStreamGenerator`: a generator is for
//! audio that does not exist yet, pushed a block at a time from a callback.
//! Every sound here is already fully decoded in memory before it plays, so a
//! generator would only add a ring buffer and a way to underrun. The whole
//! buffer goes in at once.
//!
//! Unlike the cell streamer and the skinned actor next door, this does **not**
//! bypass the scene tree. Positional audio in Godot is a property of
//! `AudioStreamPlayer3D`, and `AudioServer` has no RID call that places a
//! sound in space, so nodes are the only route to the attenuation and panning
//! the done check asks for.

use godot::classes::{audio_stream_wav, AudioStreamPlayer3D, AudioStreamWav, INode3D, Node3D};
use godot::prelude::*;
use soe_audio::{Pcm, SoundSource};
use soe_bsa::Vfs;

use crate::audio::{to_i16_bytes, to_mono, Kind, VoicePool};

/// The archives a sound can come out of, in load order. Voices are split per
/// language and only the English set is named here, which is what the
/// installed game under test ships.
const SOUND_ARCHIVES: [&str; 2] = ["Skyrim - Sounds.bsa", "Skyrim - Voices_en0.bsa"];

/// How many sounds may play at once.
///
/// Vanilla's own limit is far higher, but nothing here yet drives enough
/// emitters to need it, and a small pool makes the stealing rule observable
/// rather than theoretical.
const SLOTS: usize = 16;

/// Plays sounds resolved out of a load order, positioned in the world.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct SoundBank {
    base: Base<Node3D>,
    /// Distance in Godot units past which a sound is inaudible.
    #[export]
    max_distance: f32,
    vfs: Option<Vfs>,
    pool: VoicePool,
    players: Vec<Gd<AudioStreamPlayer3D>>,
    last_error: GString,
}

#[godot_api]
impl INode3D for SoundBank {
    fn init(base: Base<Node3D>) -> Self {
        SoundBank {
            base,
            max_distance: 2000.0,
            vfs: None,
            pool: VoicePool::new(SLOTS),
            players: Vec::new(),
            last_error: GString::new(),
        }
    }

    fn ready(&mut self) {
        // Children cannot be added before the node is in the tree, so the
        // players are built here rather than in `init`.
        for _ in 0..SLOTS {
            let mut player = AudioStreamPlayer3D::new_alloc();
            player.set_max_distance(self.max_distance);
            self.base_mut().add_child(&player);
            self.players.push(player);
        }
    }

    fn process(&mut self, _delta: f64) {
        // A player that has stopped hands its slot back. Polling rather than
        // wiring the `finished` signal keeps the release in one place and
        // costs one bool per slot per frame.
        for i in 0..self.players.len() {
            if !self.players[i].is_playing() && self.pool.peek(i).is_some() {
                self.pool.release(i);
            }
        }
    }
}

#[godot_api]
impl SoundBank {
    /// Indexes the sound archives under `data_dir`, plus `data_dir` itself as
    /// a loose overlay so a replacer beats the archive. Returns false and
    /// records the reason on failure.
    // gdext hands a `#[func]` its arguments by value, so these three take an
    // owned GString whether or not they consume it.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn open_data(&mut self, data_dir: GString) -> bool {
        let dir = std::path::PathBuf::from(data_dir.to_string());
        let mut vfs = Vfs::new();
        for name in SOUND_ARCHIVES {
            let path = dir.join(name);
            if !path.exists() {
                self.last_error =
                    GString::from(format!("no archive at {}", path.display()).as_str());
                return false;
            }
            if let Err(e) = vfs.add_archive_path(&path) {
                self.last_error = GString::from(format!("{name}: {e}").as_str());
                return false;
            }
        }
        if let Err(e) = vfs.add_loose_root(&dir) {
            self.last_error = GString::from(format!("loose overlay: {e}").as_str());
            return false;
        }
        self.vfs = Some(vfs);
        true
    }

    /// Plays a world sound such as a footstep at `position`. Returns the slot
    /// it took, or -1 when it was dropped or could not be read.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn play_effect(&mut self, data_path: GString, position: Vector3) -> i32 {
        self.play(&data_path.to_string(), position, Kind::Effect)
    }

    /// Plays a spoken line at `position`. Outranks effects when the pool is
    /// full, so a footstep cannot cut dialogue off.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn play_voice(&mut self, data_path: GString, position: Vector3) -> i32 {
        self.play(&data_path.to_string(), position, Kind::Voice)
    }

    /// How many slots are playing right now.
    #[func]
    fn playing_count(&self) -> i32 {
        self.pool.busy() as i32
    }

    /// Why the last call that returned false or -1 failed.
    #[func]
    fn last_error(&self) -> GString {
        self.last_error.clone()
    }

    fn play(&mut self, data_path: &str, position: Vector3, kind: Kind) -> i32 {
        let Some(vfs) = self.vfs.as_ref() else {
            self.last_error = GString::from("open_data has not run");
            return -1;
        };

        // The ffmpeg-backed decoder is only present when the feature is on.
        // Without it a vanilla voice line reports `Unsupported::Wma` and every
        // other format still plays, which is the crate's own arrangement.
        #[cfg(feature = "xwma-ffmpeg")]
        let wma = soe_audio::wma::Ffmpeg::new();
        #[cfg(feature = "xwma-ffmpeg")]
        let source = SoundSource::new(vfs).with_wma_decoder(&wma);
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let source = SoundSource::new(vfs);

        let sound = match source.read(data_path) {
            Ok(s) => s,
            Err(e) => {
                self.last_error = GString::from(format!("{data_path}: {e}").as_str());
                return -1;
            }
        };
        if sound.pcm.is_empty() {
            self.last_error = GString::from(format!("{data_path}: decoded to no samples").as_str());
            return -1;
        }

        let Some(slot) = self.pool.claim(kind) else {
            self.last_error =
                GString::from(format!("{data_path}: every slot holds something louder").as_str());
            return -1;
        };

        let stream = wav_from(&sound.pcm);
        let player = &mut self.players[slot];
        player.set_stream(&stream);
        player.set_position(position);
        player.play();
        slot as i32
    }
}

/// One decoded buffer as a mono 16 bit stream Godot can play.
///
/// Mono because the buffer is about to be placed in the world, and a stereo
/// image fights the position the 3D player is giving it.
fn wav_from(pcm: &Pcm) -> Gd<AudioStreamWav> {
    let mono = to_mono(pcm);
    let mut wav = AudioStreamWav::new_gd();
    wav.set_data(&PackedByteArray::from(to_i16_bytes(&mono).as_slice()));
    wav.set_format(audio_stream_wav::Format::FORMAT_16_BITS);
    wav.set_mix_rate(mono.sample_rate as i32);
    wav.set_stereo(false);
    wav
}
