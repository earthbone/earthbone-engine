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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use godot::classes::{
    audio_stream_player_3d, audio_stream_wav, AudioStreamPlayer3D, AudioStreamWav, INode3D, Node3D,
};
use godot::prelude::*;
use soe_audio::{Pcm, SoundSource};
use soe_bsa::Vfs;

use crate::audio::{to_i16_bytes, to_mono, Kind, VoicePool};
use crate::data_source::{vfs_from_loadout, DataSource};

/// How many sounds may play at once.
///
/// Vanilla's own limit is far higher, but nothing here yet drives enough
/// emitters to need it, and a small pool makes the stealing rule observable
/// rather than theoretical.
const SLOTS: usize = 16;
const CACHE_BYTES: usize = 64 * 1024 * 1024;

/// Plays sounds resolved out of a load order, positioned in the world.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct SoundBank {
    base: Base<Node3D>,
    /// Distance in Godot units past which a sound is inaudible.
    #[export]
    max_distance: f32,
    vfs: Option<Vfs>,
    source: Option<Arc<DataSource>>,
    streams: HashMap<String, (Gd<AudioStreamWav>, usize)>,
    stream_order: VecDeque<String>,
    stream_bytes: usize,
    decoded_count: u64,
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
            source: None,
            streams: HashMap::new(),
            stream_order: VecDeque::new(),
            stream_bytes: 0,
            decoded_count: 0,
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
    /// Indexes the resolved vanilla profile for `data_dir`, including only the
    /// voice archives named by its selected language. Returns false and records
    /// the reason on failure.
    // gdext hands a `#[func]` its arguments by value, so these three take an
    // owned GString whether or not they consume it.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn open_data(&mut self, data_dir: GString) -> bool {
        let dir = std::path::PathBuf::from(data_dir.to_string());
        let Some(game_dir) = dir.parent() else {
            self.last_error = GString::from("data directory has no game directory");
            return false;
        };
        let loadout = match (soe_profile::LoadoutInput {
            game_dir: game_dir.to_path_buf(),
            data_roots: vec![dir],
            plugins_txt: None,
            loadorder_txt: None,
            ini_dir: None,
            ccc: None,
        })
        .resolve()
        {
            Ok(loadout) => loadout,
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                return false;
            }
        };
        let vfs = match vfs_from_loadout(&loadout) {
            Ok(vfs) => vfs,
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                return false;
            }
        };
        self.vfs = Some(vfs);
        self.source = None;
        self.clear_streams();
        true
    }

    /// Share the host's complete archive and loose-file overlay.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn use_streamer_data(&mut self, streamer: Gd<crate::godot_layer::CellStreamerNode>) -> bool {
        let Some(source) = streamer.bind().shared_source() else {
            return false;
        };
        self.use_source(source);
        true
    }

    pub(crate) fn use_source(&mut self, source: Arc<DataSource>) {
        self.source = Some(source);
        self.vfs = None;
        self.clear_streams();
    }

    /// Test/editor entry point. Runtime callers supply draws from Game's RNG.
    #[func]
    fn play_descriptor(
        &mut self,
        file: i64,
        object: i64,
        position: Vector3,
        draws: Vector3,
    ) -> i32 {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            self.last_error = "invalid sound FormID".into();
            return -1;
        };
        let Some(source) = &self.source else {
            self.last_error = "use_streamer_data has not run".into();
            return -1;
        };
        let plan = soe_game::sound::prepare(
            &source.shared_order(),
            soe_esm::GlobalFormId { file, object },
            [draws.x, draws.y, draws.z],
            &soe_game::sound::SoundSettings::default(),
        );
        match plan {
            Ok(plan) => self.play_prepared(&plan, position),
            Err(error) => {
                self.last_error = error.to_string().as_str().into();
                -1
            }
        }
    }

    /// Number of decoder calls, for cache and installed-playback checks.
    #[func]
    fn decoded_count(&self) -> i64 {
        self.decoded_count as i64
    }

    pub(crate) fn play_prepared(
        &mut self,
        plan: &soe_game::sound::SoundPlayback,
        position: Vector3,
    ) -> i32 {
        if plan.looping != 0 {
            self.last_error = "descriptor loop/envelope playback is not connected".into();
            return -1;
        }
        self.play_with(&plan.data_path, position, Kind::Effect, Some(plan))
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
        self.play_with(data_path, position, kind, None)
    }

    fn clear_streams(&mut self) {
        self.streams.clear();
        self.stream_order.clear();
        self.stream_bytes = 0;
    }

    fn stream(&mut self, data_path: &str) -> Result<Gd<AudioStreamWav>, String> {
        let key = soe_bsa::normalize(data_path);
        if let Some((stream, _)) = self.streams.get(&key) {
            return Ok(stream.clone());
        }
        let vfs = self
            .source
            .as_ref()
            .map(|s| s.vfs())
            .or(self.vfs.as_ref())
            .ok_or_else(|| "no audio data source".to_owned())?;

        // The ffmpeg-backed decoder is only present when the feature is on.
        // Without it a vanilla voice line reports `Unsupported::Wma` and every
        // other format still plays, which is the crate's own arrangement.
        #[cfg(feature = "xwma-ffmpeg")]
        let wma = soe_audio::wma::Ffmpeg::new();
        #[cfg(feature = "xwma-ffmpeg")]
        let source = SoundSource::new(vfs).with_wma_decoder(&wma);
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let source = SoundSource::new(vfs);

        let sound = source
            .read(data_path)
            .map_err(|error| format!("{data_path}: {error}"))?;
        if sound.pcm.is_empty() {
            return Err(format!("{data_path}: decoded to no samples"));
        }
        self.decoded_count += 1;
        let stream = wav_from(&sound.pcm);
        let bytes = sound.pcm.frames().saturating_mul(2);
        if bytes <= CACHE_BYTES {
            while self.stream_bytes + bytes > CACHE_BYTES {
                let Some(oldest) = self.stream_order.pop_front() else {
                    break;
                };
                if let Some((_, size)) = self.streams.remove(&oldest) {
                    self.stream_bytes -= size;
                }
            }
            self.stream_bytes += bytes;
            self.stream_order.push_back(key.clone());
            self.streams.insert(key, (stream.clone(), bytes));
        }
        Ok(stream)
    }

    fn play_with(
        &mut self,
        data_path: &str,
        position: Vector3,
        kind: Kind,
        plan: Option<&soe_game::sound::SoundPlayback>,
    ) -> i32 {
        if self.players.len() != SLOTS {
            self.last_error = "SoundBank must be inside the scene tree".into();
            return -1;
        }
        let stream = match self.stream(data_path) {
            Ok(stream) => stream,
            Err(error) => {
                self.last_error = error.as_str().into();
                return -1;
            }
        };

        let Some(slot) = self.pool.claim(kind) else {
            self.last_error =
                GString::from(format!("{data_path}: every slot holds something louder").as_str());
            return -1;
        };

        let player = &mut self.players[slot];
        player.set_stream(&stream);
        player.set_position(position);
        player.set_pitch_scale(plan.map_or(1.0, |p| p.pitch));
        player.set_volume_db(plan.map_or(0.0, |p| {
            if p.gain > 0.0 {
                20.0 * p.gain.log10()
            } else {
                f32::NEG_INFINITY
            }
        }));
        let positional = plan.is_none_or(|p| p.output.kind == 0);
        player.set_panning_strength(if positional { 1.0 } else { 0.0 });
        let attenuates = plan.is_none_or(|p| p.output.attenuates);
        player.set_attenuation_model(if attenuates {
            audio_stream_player_3d::AttenuationModel::INVERSE_DISTANCE
        } else {
            audio_stream_player_3d::AttenuationModel::DISABLED
        });
        let range = plan.and_then(|p| p.output.attenuation.as_ref());
        player.set_unit_size(range.map_or(1.0, |a| a.min_distance / soe_physics::UNITS_PER_METER));
        player.set_max_distance(if attenuates {
            range.map_or(self.max_distance, |a| {
                a.max_distance / soe_physics::UNITS_PER_METER
            })
        } else {
            0.0
        });
        player.play();
        self.last_error = GString::new();
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
