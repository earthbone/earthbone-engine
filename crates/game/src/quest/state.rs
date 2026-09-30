// SPDX-License-Identifier: GPL-3.0-or-later

//! Quest journal and runtime state, following CommonLib TESQuest and xEdit's
//! QUST save layout. References use the live load order's identities.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;

/// One journal instance's alias names and global-value snapshots.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalInstance {
    pub aliases: BTreeMap<u32, Option<GlobalFormId>>,
    pub globals: BTreeMap<GlobalFormId, f32>,
    pub stage: u16,
    pub stage_item: i8,
}

/// Current and historical instances share one canonical collection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Journal {
    pub current: u32,
    pub instances: BTreeMap<u32, JournalInstance>,
}

impl Journal {
    /// Snapshots used by the current quest's journal text.
    pub fn globals(&self) -> Option<&BTreeMap<GlobalFormId, f32>> {
        self.instances
            .get(&self.current)
            .map(|entry| &entry.globals)
    }

    /// Update the selected instance without changing historical text.
    pub fn update_global(&mut self, id: GlobalFormId, value: f32) {
        self.instances
            .entry(self.current)
            .or_default()
            .globals
            .insert(id, value);
    }

    pub(crate) fn legacy(globals: BTreeMap<GlobalFormId, f32>) -> Self {
        let mut journal = Self::default();
        if !globals.is_empty() {
            journal.instances.insert(
                0,
                JournalInstance {
                    globals,
                    ..JournalInstance::default()
                },
            );
        }
        journal
    }
}

/// Extra creation/location references for a runtime reference alias.
/// Its actual target remains in QuestState::aliases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasMetadata {
    pub kind: u8,
    pub references: Vec<Option<GlobalFormId>>,
}

/// A remapped story-manager argument. Decoding does not dispatch the event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryParameter {
    Reference {
        kind: u32,
        reference: Option<GlobalFormId>,
    },
    Integer(u32),
}

/// Saved story-manager event context, with its original parameter order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryEvent {
    pub id: u32,
    pub code: [u8; 4],
    pub parameters: Vec<StoryParameter>,
}

/// Story Manager event context waiting for node selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryManagerEvent {
    /// Event family, such as `CraftItem`.
    pub kind: StoryManagerEventKind,
    /// Event arguments in the order defined by the CK event contract.
    pub parameters: Vec<Option<GlobalFormId>>,
}

/// Story Manager event families emitted by gameplay systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryManagerEventKind {
    /// A craft transaction created an item at a bench.
    CraftItem,
}

/// Non-target fields of the QUST runtime section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunData {
    pub unknown: u8,
    pub aliases: BTreeMap<u32, AliasMetadata>,
    pub event_flag: u8,
    pub event: Option<StoryEvent>,
}

/// Saved fields beyond the quest's existing stage, objective and alias state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Runtime {
    /// Flags other than enabled, completed, failed and active, owned by QuestState.
    pub other_flags: u16,
    pub form_flags: Option<(u32, u16)>,
    pub script_delay: Option<f32>,
    pub already_run: u8,
    pub run_data: Option<RunData>,
}

/// A quest promotion task restored from a transitional QUST flag word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleTask {
    Starting,
    Stopping,
}

/// Bits represented by QuestState's existing booleans.
pub(crate) const STATE_FLAGS: u16 = 0x0001 | 0x0002 | 0x0040 | 0x0800;
