// SPDX-License-Identifier: GPL-3.0-or-later

//! Available conversation choices for the native Godot dialogue menu.
//! Choice eligibility and response execution stay in the shared Game.

use soe_audio::LipWeightTrack;
use soe_esm::GlobalFormId;

/// Native mouth weights sampled against the active voice's playback clock.
#[derive(Default)]
pub struct DialogueMouth {
    track: Option<LipWeightTrack>,
    duration: f64,
    weights: Vec<f32>,
}

impl DialogueMouth {
    /// Start a decoded track over the duration of its voice buffer.
    pub fn start(&mut self, track: Option<LipWeightTrack>, duration: f64) {
        self.clear();
        let Some(track) = track else {
            return;
        };
        self.weights.resize(track.target_count(), 0.0);
        if duration.is_finite() && duration > 0.0 && track.frame_count() != 0 {
            self.duration = duration;
            self.track = Some(track);
        }
    }

    /// Sample the stored frame nearest at or before the voice playback time.
    pub fn sample(&mut self, seconds: f64) {
        let Some(track) = self.track.as_ref() else {
            return;
        };
        if !seconds.is_finite() || seconds < 0.0 || seconds >= self.duration {
            self.weights.fill(0.0);
            return;
        }
        let frame_offset = (seconds / self.duration * f64::from(track.frame_count())).floor();
        let native_frame = i64::from(track.first_frame()) + frame_offset as i64;
        if let Some(frame) = track.frame_at(native_frame) {
            self.weights.copy_from_slice(frame.targets);
        } else {
            self.weights.fill(0.0);
        }
    }

    /// Clear the active track and return its target weights to neutral.
    pub fn clear(&mut self) {
        self.weights.fill(0.0);
        self.track = None;
        self.duration = 0.0;
    }

    /// Current mouth weights in their stored LIP target order.
    pub fn weights(&self) -> &[f32] {
        &self.weights
    }
}

/// The active dialogue voice slot and its playback-synchronized mouth track.
#[derive(Default)]
pub struct DialogueVoice {
    slot: Option<usize>,
    mouth: DialogueMouth,
}

impl DialogueVoice {
    /// Attach a decoded track to a voice slot when playback starts.
    pub fn start(&mut self, slot: usize, track: Option<LipWeightTrack>, duration: f64) {
        self.mouth.start(track, duration);
        self.slot = Some(slot);
    }

    /// Update the mouth from the active player's current playback state.
    pub fn update(&mut self, slot: usize, playing: bool, seconds: f64) {
        if self.slot != Some(slot) {
            return;
        }
        if playing {
            self.mouth.sample(seconds);
        } else {
            self.clear();
        }
    }

    /// Stop the tracked voice and restore neutral mouth weights.
    pub fn stop(&mut self, slot: usize) {
        if self.slot == Some(slot) {
            self.clear();
        }
    }

    /// Current mouth weights in the LIP target order.
    pub fn weights(&self) -> &[f32] {
        self.mouth.weights()
    }

    fn clear(&mut self) {
        self.mouth.clear();
        self.slot = None;
    }
}

/// A resolved voice file and the speaker's position in Godot metres.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceAsset {
    pub path: String,
    pub position: [f32; 3],
}

/// Output operations needed to replace one dialogue voice with another.
pub trait VoiceOutput {
    fn play_voice(&mut self, path: &str, position: [f32; 3]) -> i32;
    fn stop_voice(&mut self, slot: i32) -> bool;
}

/// Stop the prior line before starting its replacement, retaining its slot.
pub fn replace_voice(
    output: &mut impl VoiceOutput,
    active_slot: &mut Option<i32>,
    asset: Option<&VoiceAsset>,
) {
    if let Some(slot) = active_slot.take() {
        output.stop_voice(slot);
    }
    let Some(asset) = asset else {
        return;
    };
    let slot = output.play_voice(&asset.path, asset.position);
    if slot >= 0 {
        *active_slot = Some(slot);
    }
}

/// Resolve the selected INFO's voice file for its active speaker.
///
/// Responses with a sound override use that sound form instead of a voice
/// file. The filename fields follow Mutagen's local
/// `VoiceTypeAssetLookup.GetVoiceLineFilePaths` implementation.
pub fn voice_asset(game: &mut Game, line: &Line) -> Option<VoiceAsset> {
    if line.response.sound != 0 {
        return None;
    }
    let mut world = game.world_mut();
    let response_topic = world.dialogue_topic_of(line.response_info)?;
    let topic = world.record(response_topic)?;
    let raw_quest = topic.find(soe_esm::Tag(*b"QNAM"))?;
    let raw_quest = u32::from_le_bytes(raw_quest.try_into().ok()?);
    let quest_id = world.resolve_from(response_topic, raw_quest)?;
    let quest = world.record(quest_id)?;
    let quest_name = quest.editor_id()?;
    let topic_name = topic.editor_id()?;

    let response_plugin = world
        .order()
        .file(usize::from(line.response_info.file))
        .name
        .clone();
    let response_name = format!("{:08X}", line.response_info.object);

    let (speaker_base, speaker_position) = {
        let speaker = world.reference(line.speaker)?;
        (speaker.base?, speaker.position)
    };
    let voice_owner = match world.tag_of(speaker_base)?.as_bytes() {
        b"NPC_" => world
            .npc_component_source(speaker_base, soe_game::npc::template::TRAITS)
            .ok()?,
        b"TACT" => speaker_base,
        _ => return None,
    };
    let voice_record = world.record(voice_owner)?;
    let voice_tag = if voice_record.tag.as_bytes() == b"TACT" {
        *b"VNAM"
    } else {
        *b"VTCK"
    };
    let raw_voice = voice_record.find(soe_esm::Tag(voice_tag))?;
    let raw_voice = u32::from_le_bytes(raw_voice.try_into().ok()?);
    let voice_id = world.resolve_from(voice_owner, raw_voice)?;
    let voice_name = world.record(voice_id)?.editor_id()?;
    let (quest_name, topic_name) = if quest_name.chars().count() + topic_name.chars().count() > 25 {
        if quest_name.chars().count() > 10 {
            (
                quest_name.chars().take(10).collect(),
                topic_name.chars().take(15).collect(),
            )
        } else {
            let remaining = 25 - quest_name.chars().count();
            (
                quest_name.clone(),
                topic_name.chars().take(remaining).collect(),
            )
        }
    } else {
        (quest_name.clone(), topic_name.clone())
    };
    let position = crate::units::SkyrimPos::new(
        speaker_position[0],
        speaker_position[1],
        speaker_position[2],
    )
    .to_godot();

    Some(VoiceAsset {
        path: format!(
            "Sound/Voice/{}/{}/{quest_name}_{topic_name}_{response_name}_{}.fuz",
            response_plugin, voice_name, line.response.number
        ),
        position: [position.x, position.y, position.z],
    })
}
use soe_game::dialogue::{Error, Line};
use soe_game::Game;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub topic: GlobalFormId,
    pub label: String,
}

/// Read the currently offered choices from an open conversation.
pub fn entries(game: &mut Game) -> Result<Vec<Entry>, Error> {
    let choices = game.dialogue_choices()?;
    let world = game.world();
    Ok(choices
        .available
        .into_iter()
        .map(|choice| Entry {
            topic: choice.topic,
            label: choice
                .prompt
                .or_else(|| world.name_of(choice.topic))
                .unwrap_or_else(|| format!("{}", choice.topic)),
        })
        .collect())
}

/// Start the response for a selected, currently offered topic.
pub fn choose(game: &mut Game, topic: GlobalFormId) -> Result<Option<Line>, Error> {
    game.choose_dialogue(topic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{
        Entry as PluginEntry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin,
        Record, Tag,
    };
    use soe_game::Game;

    const SPEAKER: u32 = 0x100;
    const BASE: u32 = 0x500;
    const QUEST: u32 = 0x800;
    const TOPIC: u32 = 0x900;
    const INFO: u32 = 0xA00;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn game() -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        for (form, name) in [(7, "Player"), (BASE, "Speaker")] {
            plugin.push_top(
                Record::new(Tag(*b"NPC_"), form)
                    .zsub(Tag(*b"EDID"), name)
                    .sub(Tag(*b"ACBS"), vec![0; 24]),
            );
        }
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), SPEAKER)
                .sub(Tag(*b"NAME"), BASE.to_le_bytes())
                .sub(Tag(*b"DATA"), vec![0; 24]),
        );
        plugin.push_top(
            Record::new(Tag(*b"QUST"), QUEST)
                .sub(Tag(*b"DNAM"), vec![0; 12])
                .sub(Tag(*b"NEXT"), [])
                .sub(Tag(*b"INDX"), [0, 0, 0, 0]),
        );
        plugin.push_top(
            Record::new(Tag(*b"DLBR"), TOPIC + 0x1000)
                .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
                .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
                .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
                .sub(Tag(*b"SNAM"), TOPIC.to_le_bytes()),
        );
        let topic = Record::new(Tag(*b"DIAL"), TOPIC)
            .zsub(Tag(*b"FULL"), "Ask about the road")
            .sub(Tag(*b"QNAM"), QUEST.to_le_bytes())
            .sub(Tag(*b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
            .sub(Tag(*b"DATA"), [0, 0, 0, 0])
            .sub(Tag(*b"SNAM"), *b"CUST")
            .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
        let response = Record::new(Tag(*b"INFO"), INFO)
            .sub(Tag(*b"ENAM"), [0; 4])
            .sub(Tag(*b"TRDT"), [0; 24])
            .zsub(Tag(*b"NAM1"), "The road leads north.");
        let mut group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
        group
            .entries
            .push(PluginEntry::Record(plugin.push_record(topic)));
        let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
        children
            .entries
            .push(PluginEntry::Record(plugin.push_record(response)));
        group.entries.push(PluginEntry::Group(children));
        plugin.groups.push(group);

        let mut order = LoadOrder::new();
        order.add("Dialogue.esm", plugin).unwrap();
        let mut game = Game::new(order);
        game.start_quest(id(QUEST));
        let player = game.player();
        game.begin_dialogue(id(SPEAKER), player).unwrap();
        game
    }

    #[test]
    fn dlg_10_renders_offered_topics_and_selection_uses_dialogue_api() {
        let mut game = game();
        let rows = entries(&mut game).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].topic, id(TOPIC));
        assert_eq!(rows[0].label, "Ask about the road");

        let line = choose(&mut game, rows[0].topic).unwrap().unwrap();
        assert_eq!(line.topic, id(TOPIC));
        assert_eq!(line.text.as_deref(), Some("The road leads north."));
        assert_eq!(game.dialogue_line().unwrap(), Some(line));
    }

    #[test]
    fn dlg_10_rejects_a_stale_topic_through_dialogue_api() {
        let mut game = game();
        assert!(choose(&mut game, id(TOPIC + 1)).is_err());
        assert!(game.dialogue_line().unwrap().is_none());
    }
}
