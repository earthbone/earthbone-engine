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
use std::sync::{mpsc, Arc};

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

#[derive(Clone)]
enum AudioDataSource {
    Data(Arc<DataSource>),
    Vfs(Arc<soe_bsa::Vfs>),
}

impl AudioDataSource {
    fn decode(&self, path: &str) -> Result<soe_audio::Sound, String> {
        let vfs = match self {
            Self::Data(source) => source.vfs(),
            Self::Vfs(vfs) => vfs,
        };
        #[cfg(feature = "xwma-ffmpeg")]
        let wma = soe_audio::wma::Ffmpeg::new();
        #[cfg(not(feature = "xwma-ffmpeg"))]
        let wma = soe_audio::wma::Native;
        SoundSource::new(vfs)
            .with_wma_decoder(&wma)
            .read(path)
            .map_err(|error| format!("{path}: {error}"))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum AsyncAudioKind {
    Music,
    Ambience,
    DescriptorAmbience,
}

struct AsyncAudioResult {
    kind: AsyncAudioKind,
    generation: u64,
    path: String,
    music_options: Option<(f32, f32, bool)>,
    ambience_plan: Option<soe_game::sound::SoundPlayback>,
    decoded: Result<soe_audio::Sound, String>,
}

struct CachedTrack {
    stream: Gd<AudioStreamWav>,
    bytes: usize,
}

#[derive(Debug)]
struct MusicFadeState {
    from: Option<usize>,
    to: Option<usize>,
    fade: MusicFade,
}

struct AmbientVoice {
    player: Gd<AudioStreamPlayer>,
    gain: f32,
    fade: MusicFade,
    outgoing: bool,
}

/// Plays sounds resolved out of a load order, positioned in the world.
#[derive(GodotClass)]
#[class(base = Node3D)]
pub struct SoundBank {
    base: Base<Node3D>,
    /// Distance in Godot units past which a sound is inaudible.
    #[export]
    max_distance: f32,
    vfs: Option<Arc<Vfs>>,
    source: Option<Arc<DataSource>>,
    async_sender: mpsc::Sender<AsyncAudioResult>,
    async_receiver: mpsc::Receiver<AsyncAudioResult>,
    async_workers: Vec<std::thread::JoinHandle<()>>,
    music_generation: u64,
    music_pending: Option<String>,
    music_repeat: bool,
    silent_remaining: f32,
    ambience_generation: u64,
    music_cache: HashMap<(String, AsyncAudioKind), CachedTrack>,
    music_cache_order: VecDeque<(String, AsyncAudioKind)>,
    music_cache_bytes: usize,
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
    descriptor_ambience: Vec<AmbientVoice>,
    ambience_replacement_pending: bool,
    acoustic_space: Option<soe_esm::GlobalFormId>,
    dialogue_voice: crate::dialogue_menu::DialogueVoice,
    effect_starts: u64,
    voice_starts: u64,
    music_starts: u64,
    ambience_starts: u64,
    last_error: GString,
}

impl Drop for SoundBank {
    fn drop(&mut self) {
        // SAFETY: Workers execute extension code, so join before library unload.
        for worker in self.async_workers.drain(..) {
            let _ = worker.join();
        }
    }
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
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        SoundBank::stop_voice(self, slot)
    }
}

#[godot_api]
impl INode3D for SoundBank {
    fn init(base: Base<Node3D>) -> Self {
        let (async_sender, async_receiver) = mpsc::channel();
        SoundBank {
            base,
            max_distance: 2000.0,
            vfs: None,
            source: None,
            async_sender,
            async_receiver,
            async_workers: Vec::new(),
            music_generation: 0,
            music_pending: None,
            music_repeat: false,
            silent_remaining: 0.0,
            ambience_generation: 0,
            music_cache: HashMap::new(),
            music_cache_order: VecDeque::new(),
            music_cache_bytes: 0,
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
            descriptor_ambience: Vec::new(),
            ambience_replacement_pending: false,
            acoustic_space: None,
            dialogue_voice: crate::dialogue_menu::DialogueVoice::default(),
            effect_starts: 0,
            voice_starts: 0,
            music_starts: 0,
            ambience_starts: 0,
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
        for slot in 0..2 {
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_name(&format!("MusicPlayer{slot}"));
            player.set_volume_db(-80.0);
            self.base_mut().add_child(&player);
            self.music_players.push(player);
        }
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_name("AmbiencePlayer");
        self.base_mut().add_child(&player);
        self.ambience = Some(player);
    }

    fn process(&mut self, _delta: f64) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
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
        self.advance_ambience_fades(_delta as f32);
        self.silent_remaining = (self.silent_remaining - _delta as f32).max(0.0);
        self.finish_async_audio();
    }
}

#[godot_api]
impl SoundBank {
    /// Stop the current non-positional ambience stream.
    #[func]
    pub(crate) fn stop_ambience(&mut self) {
        self.ambience_generation = self.ambience_generation.wrapping_add(1);
        if let Some(player) = &mut self.ambience {
            player.stop();
        }
        self.ambience_replacement_pending = false;
        for mut voice in self.descriptor_ambience.drain(..) {
            voice.player.stop();
            voice.player.queue_free();
        }
    }

    pub(crate) fn transition_ambience(
        &mut self,
        space: Option<soe_esm::GlobalFormId>,
        plans: &[soe_game::sound::SoundPlayback],
    ) {
        self.acoustic_space = space;
        self.ambience_generation = self.ambience_generation.wrapping_add(1);
        self.ambience_replacement_pending = true;
        if plans.is_empty() {
            self.retire_ambience();
        }
        for plan in plans {
            if !matches!(plan.looping, 0 | 8) {
                self.last_error = "ambience envelope loop mode is unsupported".into();
                continue;
            }
            let path = soe_bsa::normalize(&plan.data_path);
            let path = if path.starts_with("sound/") {
                path
            } else {
                format!("sound/{path}")
            };
            if !self.request_async_audio(
                &path,
                AsyncAudioKind::DescriptorAmbience,
                None,
                Some(plan.clone()),
            ) {
                soe_trace::error!(target: "host::audio", system = "host::audio",
                    "ambience request: {}", self.last_error);
            }
        }
    }

    /// Preview record-selected cell ambience without a running gameplay VM.
    /// Conditional descriptors return an error through `last_error`.
    #[func]
    fn preview_cell_ambience(&mut self, file: i64, object: i64, weather: i64, draw: f64) -> bool {
        let (Ok(file), Ok(object), Ok(weather)) = (
            u16::try_from(file),
            u32::try_from(object),
            u8::try_from(weather),
        ) else {
            self.last_error = "invalid cell ambience input".into();
            return false;
        };
        let Some(source) = self.source.as_ref() else {
            self.last_error = "use_streamer_data has not run".into();
            return false;
        };
        let cell = soe_esm::GlobalFormId { file, object };
        let order = source.shared_order();
        let world = soe_game::World::from_shared_order(Arc::clone(&order));
        let selection = (|| -> soe_esm::Result<_> {
            let record = world
                .record(cell)
                .ok_or_else(|| soe_esm::Error::Invalid(format!("missing CELL {cell}")))?;
            let data = soe_esm::records::cell::Cell::from_record(record, false)?;
            let space = data
                .acoustic_space
                .and_then(|raw| world.resolve_from(cell, raw));
            let sounds =
                soe_game::ambience::descriptors_for_cell(&world, cell, weather, draw as f32)?;
            let plans = sounds
                .into_iter()
                .map(|sound| {
                    soe_game::sound::prepare(
                        &order,
                        sound,
                        [draw as f32; 3],
                        &soe_game::sound::SoundSettings::default(),
                    )
                })
                .collect::<soe_esm::Result<Vec<_>>>()?;
            Ok((space, plans))
        })();
        match selection {
            Ok((space, plans)) => {
                self.transition_ambience(space, &plans);
                true
            }
            Err(error) => {
                self.last_error = error.to_string().as_str().into();
                false
            }
        }
    }

    fn retire_ambience(&mut self) {
        self.ambience_replacement_pending = false;
        for voice in &mut self.descriptor_ambience {
            if !voice.outgoing {
                let level = 10.0_f32.powf(voice.player.get_volume_db() / 20.0);
                voice.fade = MusicFade::new(level, 0.0, 1.0, false);
                voice.outgoing = true;
            }
        }
    }

    fn start_descriptor_ambience(
        &mut self,
        stream: &Gd<AudioStreamWav>,
        plan: &soe_game::sound::SoundPlayback,
    ) -> bool {
        if self.ambience_replacement_pending {
            self.retire_ambience();
        }
        let mut own_stream = AudioStreamWav::new_gd();
        own_stream.set_data(&stream.get_data());
        own_stream.set_format(stream.get_format());
        own_stream.set_mix_rate(stream.get_mix_rate());
        own_stream.set_stereo(stream.is_stereo());
        if plan.looping == 8 {
            own_stream.set_loop_mode(audio_stream_wav::LoopMode::FORWARD);
            let channels = if stream.is_stereo() { 2 } else { 1 };
            own_stream.set_loop_end(stream.get_data().len() as i32 / (2 * channels));
        }
        let mut player = AudioStreamPlayer::new_alloc();
        player.set_stream(&own_stream);
        player.set_pitch_scale(plan.pitch);
        player.set_volume_db(-80.0);
        player.set_meta("descriptor", &plan.descriptor.to_string().to_variant());
        self.base_mut().add_child(&player);
        player.play();
        self.descriptor_ambience.push(AmbientVoice {
            player,
            gain: plan.gain,
            fade: MusicFade::new(0.0, 1.0, 0.0, false),
            outgoing: false,
        });
        self.ambience_starts += 1;
        self.last_error = GString::new();
        true
    }

    fn advance_ambience_fades(&mut self, seconds: f32) {
        self.descriptor_ambience.retain_mut(|voice| {
            let (outgoing, incoming, finished) = voice.fade.advance(seconds);
            let gain = if voice.outgoing {
                outgoing
            } else {
                incoming * voice.gain
            };
            voice.player.set_volume_db(linear_to_db(gain));
            if (voice.outgoing && finished) || !voice.player.is_playing() {
                voice.player.stop();
                voice.player.queue_free();
                return false;
            }
            true
        });
    }

    /// Acoustic space chosen from the current cell's XCAS.
    #[func]
    fn acoustic_space(&self) -> GString {
        self.acoustic_space
            .map_or_else(GString::new, |id| id.to_string().as_str().into())
    }

    /// Count of current and fading ambience voices still playing.
    #[func]
    fn ambience_playing_count(&self) -> i64 {
        self.descriptor_ambience
            .iter()
            .filter(|voice| voice.player.is_playing())
            .count() as i64
    }

    pub(crate) fn music_needs_selection(&self) -> bool {
        self.music_repeat
            && self.music_pending.is_none()
            && self.silent_remaining == 0.0
            && self
                .music_current
                .is_none_or(|slot| !self.music_players[slot].is_playing())
    }

    /// Start a non-positional ambience track resolved from a MUST record.
    #[func]
    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn play_ambience(&mut self, data_path: GString) -> bool {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        if self.ambience.is_none() {
            self.last_error = "SoundBank must be inside the scene tree".into();
            return false;
        }
        self.request_async_audio(&data_path.to_string(), AsyncAudioKind::Ambience, None, None)
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
        self.vfs = Some(Arc::new(vfs));
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

    /// Current memory charged to the decoded music and ambience cache.
    #[func]
    fn music_cache_bytes(&self) -> i64 {
        self.music_cache_bytes as i64
    }

    /// Number of effect and footstep streams started since this node was added.
    #[func]
    fn effect_start_count(&self) -> i64 {
        self.effect_starts as i64
    }

    /// Number of spoken voice streams started since this node was added.
    #[func]
    fn voice_start_count(&self) -> i64 {
        self.voice_starts as i64
    }

    /// Number of music streams started since this node was added.
    #[func]
    fn music_start_count(&self) -> i64 {
        self.music_starts as i64
    }

    /// Number of music players still playing, for runtime playback checks.
    #[func]
    fn music_playing_count(&self) -> i64 {
        self.music_players
            .iter()
            .filter(|player| player.is_playing())
            .count() as i64
    }

    /// Number of ambience streams started since this node was added.
    #[func]
    fn ambience_start_count(&self) -> i64 {
        self.ambience_starts as i64
    }

    pub(crate) fn play_prepared(
        &mut self,
        plan: &soe_game::sound::SoundPlayback,
        position: Vector3,
    ) -> i32 {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
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

    pub(crate) fn voice_playing(&self, slot: i32) -> bool {
        usize::try_from(slot).is_ok_and(|slot| {
            self.pool
                .peek(slot)
                .is_some_and(|playing| playing.kind == Kind::Voice)
                && self
                    .players
                    .get(slot)
                    .is_some_and(|player| player.is_playing())
        })
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
        self.music_repeat = !track.flags.plays_one_selection;
        let Some(path) = track.path.as_deref() else {
            self.stop_music();
            self.music_repeat = !track.flags.plays_one_selection;
            self.silent_remaining = track.duration;
            return true;
        };
        self.silent_remaining = 0.0;
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
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        self.play_with(data_path, position, kind, None)
    }

    fn clear_streams(&mut self) {
        self.streams.clear();
        self.stream_order.clear();
        self.stream_bytes = 0;
        self.music_cache.clear();
        self.music_cache_order.clear();
        self.music_cache_bytes = 0;
        self.music_generation = self.music_generation.wrapping_add(1);
        self.music_pending = None;
        self.ambience_generation = self.ambience_generation.wrapping_add(1);
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
            .or(self.vfs.as_deref())
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
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        if self.music_players.len() != 2 {
            self.last_error = "SoundBank must be inside the scene tree".into();
            return false;
        }
        if !fade_in.is_finite() || !fade_out.is_finite() || fade_in < 0.0 || fade_out < 0.0 {
            self.last_error = "music fade durations must be finite and nonnegative".into();
            return false;
        }
        let key = soe_bsa::normalize(data_path);
        let cache_key = (key.clone(), AsyncAudioKind::Music);
        if self.music_pending.as_deref() == Some(&key) {
            return true;
        }
        if self.music_current.is_some_and(|slot| {
            self.music_paths[slot].as_deref() == Some(key.as_str())
                && self.music_players[slot].is_playing()
        }) {
            self.music_generation = self.music_generation.wrapping_add(1);
            self.music_pending = None;
            return true;
        }
        if let Some(cached) = self.music_cache.get(&cache_key) {
            let stream = cached.stream.clone();
            self.music_generation = self.music_generation.wrapping_add(1);
            return self.start_music_stream(data_path, &stream, fade_in, fade_out, abrupt);
        }
        self.request_async_audio(
            data_path,
            AsyncAudioKind::Music,
            Some((fade_in, fade_out, abrupt)),
            None,
        )
    }

    fn start_music_stream(
        &mut self,
        data_path: &str,
        stream: &Gd<AudioStreamWav>,
        fade_in: f32,
        fade_out: f32,
        abrupt: bool,
    ) -> bool {
        self.music_pending = None;
        let from = self.music_current;
        let outgoing_fade = self.music_current_fade_out;
        let to = from.map_or(0, |slot| 1 - slot);
        // This is the older outgoing player from an interrupted crossfade.
        // Stop it before reusing the slot so rapid requests never leave stale
        // music playing behind the latest selection.
        self.music_players[to].stop();
        self.music_paths[to] = Some(soe_bsa::normalize(data_path));
        let from_level = from.map_or(0.0, |slot| {
            10.0_f32.powf(self.music_players[slot].get_volume_db() / 20.0)
        });
        let target = &mut self.music_players[to];
        target.set_stream(stream);
        target.set_volume_db(-80.0);
        target.play();
        self.music_starts += 1;
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

    pub(crate) fn stop_music(&mut self) {
        self.music_repeat = false;
        self.music_pending = None;
        self.silent_remaining = 0.0;
        self.music_generation = self.music_generation.wrapping_add(1);
        if self.music_players.len() != 2 {
            return;
        }
        let from = self.music_current;
        if from.is_none() {
            return;
        }
        if let Some(previous) = self.music_fade.take().and_then(|fade| fade.from) {
            if Some(previous) != from {
                self.music_players[previous].stop();
                self.music_paths[previous] = None;
            }
        }
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

    fn request_async_audio(
        &mut self,
        path: &str,
        kind: AsyncAudioKind,
        music_options: Option<(f32, f32, bool)>,
        ambience_plan: Option<soe_game::sound::SoundPlayback>,
    ) -> bool {
        let key = soe_bsa::normalize(path);
        let cache_key = (key.clone(), kind);
        let generation = match kind {
            AsyncAudioKind::Music => {
                self.music_pending = Some(key.clone());
                self.music_generation = self.music_generation.wrapping_add(1);
                self.music_generation
            }
            AsyncAudioKind::Ambience => {
                self.ambience_generation = self.ambience_generation.wrapping_add(1);
                self.ambience_generation
            }
            AsyncAudioKind::DescriptorAmbience => self.ambience_generation,
        };
        if let Some(cached) = self.music_cache.get(&cache_key) {
            let stream = cached.stream.clone();
            return match kind {
                AsyncAudioKind::Music => {
                    let (fade_in, fade_out, abrupt) = music_options.unwrap_or((0.0, 0.0, true));
                    self.start_music_stream(path, &stream, fade_in, fade_out, abrupt)
                }
                AsyncAudioKind::Ambience => self.start_ambience_stream(&stream),
                AsyncAudioKind::DescriptorAmbience => ambience_plan
                    .as_ref()
                    .is_some_and(|plan| self.start_descriptor_ambience(&stream, plan)),
            };
        }
        let Some(source) = self
            .source
            .as_ref()
            .map(|source| AudioDataSource::Data(Arc::clone(source)))
            .or_else(|| {
                self.vfs
                    .as_ref()
                    .map(|vfs| AudioDataSource::Vfs(Arc::clone(vfs)))
            })
        else {
            self.last_error = "no audio data source".into();
            return false;
        };
        let sender = self.async_sender.clone();
        let requested_path = path.to_owned();
        let result_path = key;
        let worker = std::thread::Builder::new()
            .name("soe-audio-decode".to_owned())
            .spawn(move || {
                let decoded = source.decode(&requested_path);
                let _ = sender.send(AsyncAudioResult {
                    kind,
                    generation,
                    path: result_path,
                    music_options,
                    ambience_plan,
                    decoded,
                });
            });
        match worker {
            Ok(worker) => self.async_workers.push(worker),
            Err(error) => {
                self.last_error = GString::from(error.to_string().as_str());
                return false;
            }
        }
        self.last_error = GString::new();
        true
    }

    fn finish_async_audio(&mut self) {
        let mut index = 0;
        while index < self.async_workers.len() {
            if self.async_workers[index].is_finished() {
                let _ = self.async_workers.swap_remove(index).join();
            } else {
                index += 1;
            }
        }
        while let Ok(result) = self.async_receiver.try_recv() {
            if result
                .decoded
                .as_ref()
                .is_ok_and(|sound| !sound.pcm.is_empty())
            {
                self.decoded_count += 1;
            }
            let current_generation = match result.kind {
                AsyncAudioKind::Music => self.music_generation,
                AsyncAudioKind::Ambience | AsyncAudioKind::DescriptorAmbience => {
                    self.ambience_generation
                }
            };
            if result.generation != current_generation {
                continue;
            }
            if result.kind == AsyncAudioKind::Music {
                self.music_pending = None;
            }
            let sound = match result.decoded {
                Ok(sound) if !sound.pcm.is_empty() => sound,
                Ok(_) => {
                    if result.kind == AsyncAudioKind::Music {
                        self.music_repeat = false;
                    }
                    self.last_error =
                        GString::from(format!("{}: decoded to no samples", result.path).as_str());
                    soe_trace::error!(target: "host::audio", system = "host::audio",
                        "audio decode: {}", self.last_error);
                    continue;
                }
                Err(error) => {
                    if result.kind == AsyncAudioKind::Music {
                        self.music_repeat = false;
                    }
                    self.last_error = GString::from(error.as_str());
                    soe_trace::error!(target: "host::audio", system = "host::audio",
                        "audio decode: {}", self.last_error);
                    continue;
                }
            };
            let stream = match result.kind {
                AsyncAudioKind::Music => wav_from_stereo(&sound.pcm),
                AsyncAudioKind::Ambience | AsyncAudioKind::DescriptorAmbience => {
                    wav_from(&sound.pcm)
                }
            };
            let bytes = sound
                .pcm
                .samples
                .len()
                .saturating_mul(std::mem::size_of::<i16>());
            if bytes <= CACHE_BYTES {
                while self.music_cache_bytes.saturating_add(bytes) > CACHE_BYTES {
                    let Some(oldest) = self.music_cache_order.pop_front() else {
                        break;
                    };
                    if let Some(old) = self.music_cache.remove(&oldest) {
                        self.music_cache_bytes = self.music_cache_bytes.saturating_sub(old.bytes);
                    }
                }
                let cache_key = (result.path.clone(), result.kind);
                if let Some(previous) = self.music_cache.remove(&cache_key) {
                    self.music_cache_bytes = self.music_cache_bytes.saturating_sub(previous.bytes);
                    self.music_cache_order.retain(|key| key != &cache_key);
                }
                self.music_cache_bytes = self.music_cache_bytes.saturating_add(bytes);
                self.music_cache_order.push_back(cache_key.clone());
                self.music_cache.insert(
                    cache_key,
                    CachedTrack {
                        stream: stream.clone(),
                        bytes,
                    },
                );
            }
            let started = match result.kind {
                AsyncAudioKind::Music => {
                    let (fade_in, fade_out, abrupt) =
                        result.music_options.unwrap_or((0.0, 0.0, true));
                    self.start_music_stream(&result.path, &stream, fade_in, fade_out, abrupt)
                }
                AsyncAudioKind::Ambience => self.start_ambience_stream(&stream),
                AsyncAudioKind::DescriptorAmbience => result
                    .ambience_plan
                    .as_ref()
                    .is_some_and(|plan| self.start_descriptor_ambience(&stream, plan)),
            };
            if !started {
                self.last_error = "SoundBank must be inside the scene tree".into();
            }
        }
    }

    fn start_ambience_stream(&mut self, stream: &Gd<AudioStreamWav>) -> bool {
        let Some(player) = &mut self.ambience else {
            return false;
        };
        player.stop();
        player.set_stream(stream);
        player.play();
        self.ambience_starts += 1;
        true
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
        player.set_global_position(position);
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
        let _entity = soe_trace::EntityScope::new(
            || {
                soe_trace::Entity::new(
                    "output",
                    if kind == Kind::Voice {
                        "audio_voices"
                    } else {
                        "audio_effects"
                    },
                    "<host>",
                    "started",
                )
            },
            1,
        );
        player.play();
        if kind == Kind::Voice {
            self.voice_starts += 1;
            self.dialogue_voice.start(slot, lip, duration);
        } else {
            self.effect_starts += 1;
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
