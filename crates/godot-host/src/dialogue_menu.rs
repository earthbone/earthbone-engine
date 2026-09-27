// SPDX-License-Identifier: GPL-3.0-or-later

//! Available conversation choices for the native Godot dialogue menu.
//! Choice eligibility and response execution stay in the shared Game.

use soe_esm::GlobalFormId;
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
