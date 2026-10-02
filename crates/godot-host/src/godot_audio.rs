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
    audio_stream_player_3d, audio_stream_wav, AudioStreamPlayer, AudioStreamPlayer3D,
    AudioStreamWav, INode3D, Node3D,
};
use godot::prelude::*;
use soe_audio::{LipWeightTrack, Pcm, SoundSource};
use soe_bsa::Vfs;

use crate::audio::{to_i16_bytes, to_mono, Kind, MusicFade, VoicePool};
use crate::data_source::{vfs_from_loadout, DataSource};

/// How many sounds may play at once.
///
/// Vanilla's own limit is far higher, but nothing here yet drives enough
/// emitters to need it, and a small pool makes the stealing rule observable
/// rather than theoretical.
const SLOTS: usize = 16;
const CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug)]
struct MusicFadeState {
    from: Option<usize>,
    to: Option<usize>,
    fade: MusicFade,
}

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
    streams: HashMap<String, (Gd<AudioStreamWav>, usize, f64, Option<LipWeightTrack>)>,
    stream_order: VecDeque<String>,
    stream_bytes: usize,
    decoded_count: u64,
    pool: VoicePool,
    players: Vec<Gd<AudioStreamPlayer3D>>,
    music_players: Vec<Gd<AudioStreamPlayer>>,
    music_paths: [Option<String>; 2],
    music_current: Option<usize>,
    music_current_fade_out: f32,
    music_fade: Option<MusicFadeState>,
    ambience: Option<Gd<AudioStreamPlayer>>,
    dialogue_voice: crate::dialogue_menu::DialogueVoice,
    last_error: GString,
}

impl crate::dialogue_menu::VoiceOutput for SoundBank {
    fn play_voice(&mut self, path: &str, position: [f32; 3]) -> i32 {
        SoundBank::play_voice(
            self,
            GString::from(path),
            Vector3::new(position[0], position[1], position[2]),
        )
    }

    fn stop_voice(&mut self, slot: i32) -> bool {
        SoundBank::stop_voice(self, slot)
    }
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
            music_players: Vec::new(),
            music_paths: [None, None],
            music_current: None,
            music_current_fade_out: 0.0,
            music_fade: None,
            ambience: None,
            dialogue_voice: crate::dialogue_menu::DialogueVoice::default(),
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
        for _ in 0..2 {
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_volume_db(-80.0);
            self.base_mut().add_child(&player);
            self.music_players.push(player);
        }
        let player = AudioStreamPlayer::new_alloc();
        self.base_mut().add_child(&player);
        self.ambience = Some(player);
    }

    fn process(&mut self, _delta: f64) {
        // A player that has stopped hands its slot back. Polling rather than
        // wiring the `finished` signal keeps the release in one place and
        // costs one bool per slot per frame.
        for i in 0..self.players.len() {
            let playing = self.players[i].is_playing();
            if !playing && self.pool.peek(i).is_some() {
                self.pool.release(i);
            }
            self.dialogue_voice.update(
                i,
                playing,
                f64::from(self.players[i].get_playback_position()),
            );
        }
        self.advance_music_fade(_delta as f32);
    }
}

#[godot_api]
impl SoundBank {
    /// Stop the current non-positional ambience stream.
    pub(crate) fn stop_ambience(&mut self) {
        if let Some(player) = &mut self.ambience {
            player.stop();
        }
    }

    /// Start a non-positional ambience track resolved from a MUST record.
    pub(crate) fn play_ambience(&mut self, data_path: &str) -> bool {
        let stream = match self.stream(data_path) {
            Ok((stream, _, _)) => stream,
            Err(error) => {
                self.last_error = error.as_str().into();
                return false;
            }
        };
        let Some(player) = &mut self.ambience else {
            self.last_error = "SoundBank must be inside the scene tree".into();
            return false;
        };
        player.stop();
        player.set_stream(&stream);
        player.play();
        self.last_error = GString::new();
        true
    }

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
    pub(crate) fn play_voice(&mut self, data_path: GString, position: Vector3) -> i32 {
        self.play(&data_path.to_string(), position, Kind::Voice)
    }

    /// Stops a voice slot while leaving effect playback untouched.
    #[func]
    pub(crate) fn stop_voice(&mut self, slot: i32) -> bool {
        let Ok(slot) = usize::try_from(slot) else {
            return false;
        };
        if self
            .pool
            .peek(slot)
            .is_none_or(|playing| playing.kind != Kind::Voice)
        {
            return false;
        }
        let Some(player) = self.players.get_mut(slot) else {
            return false;
        };
        player.stop();
        self.pool.release(slot);
        self.dialogue_voice.stop(slot);
        true
    }

    /// Play a decoded music file with the supplied record fade durations.
    /// A new request cancels the older outgoing slot before starting its own.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    fn play_music_track(
        &mut self,
        data_path: GString,
        fade_in: f64,
        fade_out: f64,
        abrupt: bool,
    ) -> bool {
        self.begin_music(
            &data_path.to_string(),
            fade_in as f32,
            fade_out as f32,
            abrupt,
        )
    }

    /// Queue a selection produced by `soe_game::music::select` on the shared
    /// game. Kept as a Rust entry point because Game is owned by PlayerNode.
    pub(crate) fn play_selected_music(&mut self, track: &soe_game::music::MusicTrack) -> bool {
        let Some(path) = track.path.as_deref() else {
            self.stop_music();
            return true;
        };
        self.begin_music(
            path,
            track.fade_in,
            track.fade_out,
            track.flags.abrupt_transition,
        )
    }

    /// How many slots are playing right now.
    #[func]
    fn playing_count(&self) -> i32 {
        self.pool.busy() as i32
    }

    /// Mouth target weights from the active dialogue voice, in LIP order.
    #[func]
    fn dialogue_mouth_weights(&self) -> PackedFloat32Array {
        PackedFloat32Array::from(self.dialogue_voice.weights().to_vec())
    }

    /// Why the last call that returned false or -1 failed.
    #[func]
    pub(crate) fn last_error(&self) -> GString {
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

    fn stream(
        &mut self,
        data_path: &str,
    ) -> Result<(Gd<AudioStreamWav>, f64, Option<LipWeightTrack>), String> {
        let key = soe_bsa::normalize(data_path);
        if let Some((stream, _, duration, lip)) = self.streams.get(&key) {
            return Ok((stream.clone(), *duration, lip.clone()));
        }
        let vfs = self
            .source
            .as_ref()
            .map(|s| s.vfs())
            .or(self.vfs.as_ref())
            .ok_or_else(|| "no audio data source".to_owned())?;

        // The pure Rust decoder handles vanilla xWMA without an external
        // process. The optional ffmpeg decoder remains useful for comparison.
        #[cfg(feature = "xwma-ffmpeg")]
        let wma = soe_audio::wma::Ffmpeg::new();
        #[cfg(feature = "xwma-ffmpeg")]
        let source = SoundSource::new(vfs).with_wma_decoder(&wma);
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let wma = soe_audio::wma::Native;
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let source = SoundSource::new(vfs).with_wma_decoder(&wma);

        let sound = source
            .read(data_path)
            .map_err(|error| format!("{data_path}: {error}"))?;
        if sound.pcm.is_empty() {
            return Err(format!("{data_path}: decoded to no samples"));
        }
        let duration = sound.pcm.duration_seconds();
        let lip = sound.lip.as_deref().and_then(|bytes| {
            soe_audio::Lip::parse(bytes)
                .and_then(|lip| lip.decode())
                .ok()
        });
        self.decoded_count += 1;
        let stream = wav_from(&sound.pcm);
        let pcm_bytes = sound.pcm.frames().saturating_mul(2);
        let lip_bytes = lip.as_ref().map_or(0, |track| {
            usize::try_from(track.frame_count())
                .unwrap_or(usize::MAX)
                .saturating_mul(track.target_count().saturating_add(track.gesture_count()))
                .saturating_mul(std::mem::size_of::<f32>())
        });
        let bytes = pcm_bytes.saturating_add(lip_bytes);
        if bytes <= CACHE_BYTES {
            while self.stream_bytes + bytes > CACHE_BYTES {
                let Some(oldest) = self.stream_order.pop_front() else {
                    break;
                };
                if let Some((_, size, _, _)) = self.streams.remove(&oldest) {
                    self.stream_bytes -= size;
                }
            }
            self.stream_bytes += bytes;
            self.stream_order.push_back(key.clone());
            self.streams
                .insert(key, (stream.clone(), bytes, duration, lip.clone()));
        }
        Ok((stream, duration, lip))
    }

    fn begin_music(&mut self, data_path: &str, fade_in: f32, fade_out: f32, abrupt: bool) -> bool {
        if self.music_players.len() != 2 {
            self.last_error = "SoundBank must be inside the scene tree".into();
            return false;
        }
        if !fade_in.is_finite() || !fade_out.is_finite() || fade_in < 0.0 || fade_out < 0.0 {
            self.last_error = "music fade durations must be finite and nonnegative".into();
            return false;
        }
        let stream = match self.music_stream(data_path) {
            Ok(stream) => stream,
            Err(error) => {
                self.last_error = error.as_str().into();
                return false;
            }
        };
        if self
            .music_current
            .is_some_and(|slot| self.music_paths[slot].as_deref() == Some(data_path))
        {
            return true;
        }
        let from = self.music_current;
        let outgoing_fade = self.music_current_fade_out;
        let to = from.map_or(0, |slot| 1 - slot);
        // This is the older outgoing player from an interrupted crossfade.
        // Stop it before reusing the slot so rapid requests never leave stale
        // music playing behind the latest selection.
        self.music_players[to].stop();
        self.music_paths[to] = Some(data_path.to_owned());
        let from_level = from.map_or(0.0, |slot| {
            10.0_f32.powf(self.music_players[slot].get_volume_db() / 20.0)
        });
        let target = &mut self.music_players[to];
        target.set_stream(&stream);
        target.set_volume_db(-80.0);
        target.play();
        self.music_current = Some(to);
        self.music_current_fade_out = fade_out;
        self.music_fade = Some(MusicFadeState {
            from,
            to: Some(to),
            fade: MusicFade::new(from_level, fade_in, outgoing_fade, abrupt),
        });
        if abrupt {
            self.advance_music_fade(0.0);
        }
        self.last_error = GString::new();
        true
    }

    fn stop_music(&mut self) {
        if self.music_players.len() != 2 {
            return;
        }
        let from = self.music_current;
        let from_level = from.map_or(0.0, |slot| {
            10.0_f32.powf(self.music_players[slot].get_volume_db() / 20.0)
        });
        let fade_out = self.music_current_fade_out;
        self.music_current = None;
        self.music_current_fade_out = 0.0;
        self.music_fade = Some(MusicFadeState {
            from,
            to: None,
            fade: MusicFade::new(from_level, 0.0, fade_out, false),
        });
    }

    fn advance_music_fade(&mut self, seconds: f32) {
        let Some(state) = self.music_fade.as_mut() else {
            return;
        };
        let (from_level, to_level, finished) = state.fade.advance(seconds);
        if let Some(slot) = state.from {
            self.music_players[slot].set_volume_db(linear_to_db(from_level));
        }
        if let Some(slot) = state.to {
            self.music_players[slot].set_volume_db(linear_to_db(to_level));
        }
        if finished {
            if let Some(slot) = state.from {
                self.music_players[slot].stop();
                self.music_paths[slot] = None;
            }
            if let Some(slot) = state.to {
                self.music_players[slot].set_volume_db(0.0);
            }
            self.music_fade = None;
        }
    }

    fn music_stream(&mut self, data_path: &str) -> Result<Gd<AudioStreamWav>, String> {
        let vfs = self
            .source
            .as_ref()
            .map(|source| source.vfs())
            .or(self.vfs.as_ref())
            .ok_or_else(|| "no audio data source".to_owned())?;
        #[cfg(feature = "xwma-ffmpeg")]
        let wma = soe_audio::wma::Ffmpeg::new();
        #[cfg(feature = "xwma-ffmpeg")]
        let decoder = SoundSource::new(vfs).with_wma_decoder(&wma);
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let decoder = SoundSource::new(vfs);
        let sound = decoder
            .read(data_path)
            .map_err(|error| format!("{data_path}: {error}"))?;
        if sound.pcm.is_empty() {
            return Err(format!("{data_path}: decoded to no samples"));
        }
        self.decoded_count += 1;
        Ok(wav_from_stereo(&sound.pcm))
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
        let (stream, duration, lip) = match self.stream(data_path) {
            Ok(decoded) => decoded,
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
        if kind == Kind::Voice {
            self.dialogue_voice.start(slot, lip, duration);
        }
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

fn wav_from_stereo(pcm: &Pcm) -> Gd<AudioStreamWav> {
    let mut wav = AudioStreamWav::new_gd();
    wav.set_data(&PackedByteArray::from(to_i16_bytes(pcm).as_slice()));
    wav.set_format(audio_stream_wav::Format::FORMAT_16_BITS);
    wav.set_mix_rate(pcm.sample_rate as i32);
    wav.set_stereo(pcm.channels > 1);
    wav
}

fn linear_to_db(gain: f32) -> f32 {
    if gain <= 0.0001 {
        -80.0
    } else {
        20.0 * gain.log10()
    }
}
