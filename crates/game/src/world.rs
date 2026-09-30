// SPDX-License-Identifier: GPL-3.0-or-later

//! The runtime form table. Every game object is a `GlobalFormId` from the
//! load order, and the state a running game adds to a form (a reference's
//! position and enable state, a quest's stage, a global's value) lives here
//! keyed by that id. Base data is read from the winning record on first use,
//! so the table stays a view over the load order rather than a copy of it.
//!
//! Natives run inside the VM and cannot touch the VM, so anything a native
//! wants the VM to do (deliver an event, call a fragment) is queued in
//! `pending` and the [`crate::Game`] loop drains it.
//!
//! This file holds the state types, the `World` struct and its constructor.
//! The methods sit in one child module per concern, each an `impl World`:
//!
//! - `lookup`: editor ids, records, run-time FormIDs, keywords, names, classes.
//! - `objects`: the handle to VM object table.
//! - `refs`: reference state, placement, the player, inventory, activation.
//! - `actors`: the `NPC_` view, actor state and actor values.
//! - `quests`: quest state, stages, objectives, aliases.
//! - `forms`: globals, form lists, leveled lists, locations, factions, actor
//!   bases, cells, game settings.
//! - `events`: what natives raise for registered listeners, scenes, sounds.
//! - `timers`: `RegisterForUpdate` timers.
//! - `rng`: the random number stream.
//! - `driver`: what the package driver reads and calls.
//! - `save`: the `SOEW` save format.
//! - `hash`: the determinism hash.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use soe_esm::records::{Text, Transform};
use soe_esm::strings::{StringTable, StringTableKind};
use soe_esm::{GlobalFormId, LoadOrder, RecordRef, Tag};
use soe_papyrus::{ObjectId, Value};

use crate::clock::GameClock;
use crate::quest::Quest;

mod actor_seeds;
mod actor_spells;
mod actors;
mod bleedout;
mod calendar;
mod combat;
mod contact;
mod crime;
mod death;
mod detection;
mod dialogue_scene_restore;
mod doorway;
mod driver;
mod encounter_zones;
mod events;
pub use events::EffectShaderRequest;
mod forms;
mod furniture;
pub mod hash;
mod inventory;
mod inventory_restore;
mod spell_hand;
mod weapon_hands;
pub use inventory_restore::InventoryRestoreReport;
pub mod image_space;
mod item_save;
mod list_restore;
mod localization;
mod locks;
mod lookup;
pub mod magic;
mod npc_restore;
mod objects;
mod outfits;
mod package_navmesh_restore;
pub use npc_restore::NpcRestoreReport;
mod record_change_restore;
mod record_overrides;
mod skins;
mod temporary_npc;
pub use record_overrides::NpcRecordExtra;
pub use record_overrides::{NavmeshRecordExtra, PackRecordExtra};
pub use record_overrides::{UnresolvedRecord, UnresolvedRecordId};
mod projectiles;
mod quest_restore;
mod quest_save;
mod quests;
pub use quests::AliasResolutionError;
mod story_manager;
pub use story_manager::StoryError;
mod record_save;
pub use quest_restore::QuestRestoreReport;
mod reference_extras;
mod reference_restore;
mod refs;
pub use reference_restore::{MissingPlacementCell, ReferenceRemainder, ReferenceRestoreReport};
pub use refs::RefPlacement;
mod leveled_items;
mod rng;
pub(crate) mod save;
mod save_section_registry;
mod save_sections;
mod template_restore;
mod templates;
pub use template_restore::TemplateRestoreReport;
mod timers;
mod transition;

pub use contact::{ContactActor, DEFAULT_COMBAT_DISTANCE};
pub use dialogue_scene_restore::DialogueSceneRestoreReport;
pub use doorway::{Doorway, InteriorDestination, Side, Threshold, THRESHOLD_BAND, UNITS_PER_METRE};
pub use projectiles::RangedEquipment;
pub use reference_extras::{ReferenceExtrasRestoreReport, ReferenceLock};
pub use save::{SAVE_MAGIC, SAVE_VERSION};
pub use transition::CellTransition;

/// Created references and temporary bases share an internal namespace outside
/// the plugin positions. Runtime FormIDs still use the `FF` prefix.
pub const DYNAMIC_FILE: u16 = soe_esm::CREATED_FILE;
pub const PLAYER_OBJECT: u32 = 0x14;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// What a VM object stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    Form(GlobalFormId),
    Alias {
        quest: GlobalFormId,
        id: u32,
    },
    /// One application of a magic effect, distinct from its MGEF definition.
    ActiveEffect(magic::EffectId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefState {
    pub base: Option<GlobalFormId>,
    pub cell: Option<GlobalFormId>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: f32,
    /// Runtime record flags changed by Base Object Swapper.
    pub record_flags: u32,
    pub enabled: bool,
    pub persistent: bool,
    pub deleted: bool,
    pub activation_blocked: bool,
    pub opened: bool,
    pub locked: bool,
    pub lock_level: i32,
    /// Saved key and lock metadata. None keeps the authored key.
    pub lock_extra: Option<ReferenceLock>,
    pub display_name: Option<String>,
    pub inventory: crate::inventory::Inventory,
    /// Loose collectible quantity and extras, moved out when put into an inventory.
    pub item: Option<crate::inventory::Stack>,
    /// `(keyword, reference)` per XLKR.
    pub linked: Vec<(Option<GlobalFormId>, GlobalFormId)>,
    /// A door's far side: the other door and where to land.
    pub teleport: Option<(GlobalFormId, Transform)>,
    pub is_actor: bool,
    /// `XOWN`: an `NPC_` or a `FACT`.
    pub owner: Option<GlobalFormId>,
    /// ExtraRank and ExtraGlobal retained for ownership rules.
    pub owner_rank: Option<i32>,
    pub owner_global: Option<GlobalFormId>,
    /// `XLCN`, the reference's own location when the record names one.
    pub location: Option<GlobalFormId>,
    /// A map marker shown on the player's map (`AddToMap`).
    pub on_map: bool,
    pub fast_travel: bool,
    pub destroyed: bool,
    /// Current destruction stage, with -1 meaning undamaged.
    pub destruction_stage: i32,
    pub ignore_friendly_hits: bool,
    /// Animation graph variables set through `SetAnimationVariable*`, keyed
    /// by lowercase name. The graph has one value per name, the three
    /// typed pairs read and write the same store.
    pub anim_floats: BTreeMap<String, f32>,
    pub anim_ints: BTreeMap<String, i32>,
    pub anim_bools: BTreeMap<String, bool>,
    /// `SetAlpha`, the fade the Dawnguard and Dragonborn scripts use.
    pub alpha: f32,
}

impl RefState {
    fn new(base: Option<GlobalFormId>) -> Self {
        RefState {
            base,
            cell: None,
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            record_flags: 0,
            enabled: true,
            persistent: false,
            deleted: false,
            activation_blocked: false,
            opened: false,
            locked: false,
            lock_level: 0,
            lock_extra: None,
            display_name: None,
            inventory: crate::inventory::Inventory::default(),
            item: None,
            linked: Vec::new(),
            teleport: None,
            is_actor: false,
            owner: None,
            owner_rank: None,
            owner_global: None,
            location: None,
            on_map: false,
            fast_travel: false,
            destroyed: false,
            destruction_stage: -1,
            ignore_friendly_hits: false,
            anim_floats: BTreeMap::new(),
            anim_ints: BTreeMap::new(),
            anim_bools: BTreeMap::new(),
            alpha: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActorState {
    /// Current values keyed by lowercase actor value name.
    pub values: crate::actor_values::Values,
    pub dead: bool,
    pub in_combat: bool,
    pub teammate: bool,
    pub ghost: bool,
    pub essential: bool,
    pub protected: bool,
    pub level: u32,
    pub race: Option<GlobalFormId>,
    pub unconscious: bool,
    pub doing_favor: bool,
    pub factions: HashMap<GlobalFormId, i32>,
    pub spells: BTreeSet<GlobalFormId>,
    pub equipped: BTreeSet<GlobalFormId>,
    /// Explicit weapon slots. The equipment set retains armor and spell membership.
    pub left_weapon: Option<GlobalFormId>,
    pub right_weapon: Option<GlobalFormId>,
    /// Explicit casting-hand spell slots, set through `World::equip_hand_spell`.
    /// Independent of `left_weapon`/`right_weapon`, both a weapon and a spell
    /// slot may hold a value for the same hand at once.
    pub left_spell: Option<GlobalFormId>,
    pub right_spell: Option<GlobalFormId>,
    /// The package driver's state, see `crate::ai`.
    pub ai: crate::ai::AiState,
    pub perks: BTreeSet<GlobalFormId>,
    pub alerted: bool,
    pub sneaking: bool,
    /// The actor's current swimming state, supplied by runtime movement.
    pub swimming: bool,
    /// The actor's current running state, supplied by runtime movement.
    pub running: bool,
    /// Whether the actor is currently riding a mount.
    pub riding_mount: bool,
    pub weapon_drawn: bool,
    /// The host animation graph's current block stance. Unlike the other
    /// combat stance flags above, this is not part of saved state: the host
    /// re-signals it from the live graph after load, through
    /// `World::set_blocking`.
    pub blocking: bool,
    pub restrained: bool,
    pub arrested: bool,
    pub attack_on_sight: bool,
    pub dont_move: bool,
    pub allow_pc_dialogue: bool,
    /// The active exclusive `DLBR`, retained when conversation closes.
    pub exclusive_branch: Option<GlobalFormId>,
    /// Relationship rank per other actor, `SetRelationshipRank` values.
    pub relationships: BTreeMap<GlobalFormId, i32>,
    pub combat_target: Option<GlobalFormId>,
    pub crime_faction: Option<GlobalFormId>,
    pub outfit: Option<GlobalFormId>,
    pub sleep_outfit: Option<GlobalFormId>,
    pub killer: Option<GlobalFormId>,
    /// Behavior graph floats set through `SetSubGraphFloatVariable`, keyed
    /// by lowercase name.
    pub graph_floats: BTreeMap<String, f32>,
    /// `SetAllowFlying`, the Dawnguard dragon riding flag.
    pub allow_flying: bool,
    /// Between `StartDeferredKill` and `EndDeferredKill` death waits.
    pub deferred_kill: bool,
    /// The furniture reference and zero-based marker currently reserved by
    /// this actor. The marker remains reserved until the actor stands.
    pub furniture_seat: Option<(GlobalFormId, u16)>,
    /// The last state `SendVampirismStateChanged` announced for this actor.
    pub vampire: bool,
    /// The last state `SendLycanthropyStateChanged` announced.
    pub werewolf: bool,
    /// `SetEyeTexture`, the Dawnguard vampire eyes.
    pub eye_texture: Option<GlobalFormId>,
    /// Player character head parts selected in the race menu, keyed by HDPT type.
    pub head_parts: BTreeMap<u32, GlobalFormId>,
    /// Player character morph controls selected in the race menu.
    pub face_morphs: BTreeMap<String, f32>,
    /// `SetCriticalStage`, the death-by-critical stages.
    pub critical_stage: i32,
    /// `SetNoBleedoutRecovery`: this actor stays down once it bleeds out.
    pub no_bleedout_recovery: bool,
    /// `EnableAI(false)`. Stored inverted so a default actor thinks.
    pub ai_disabled: bool,
    /// The base form `AttachAshPile` left on this actor.
    pub ash_pile: Option<GlobalFormId>,
    pub shouts: BTreeSet<GlobalFormId>,
    pub equipped_shout: Option<GlobalFormId>,
    /// Real-time deadline for the stagger animation and input lock a melee
    /// hit above `fStaggerThreshold` sets, `None` when not staggered.
    /// Compared against `World::clock.real_seconds`, the lazy expiry
    /// `ai::AiState::wait_until` also uses, see `World::is_staggered`.
    pub stagger_end_time: Option<f64>,
    /// Perpetrators this actor has witnessed committing a crime against a
    /// faction that tracks crime. The bounty itself lives on
    /// `FactionState`, this is the actor's own memory of who it saw do it.
    /// See `World::record_witnessed_crime`.
    pub witnessed_crimes: BTreeSet<GlobalFormId>,
    /// Partial use experience for each actor skill, keyed by lowercase name.
    pub skill_experience: BTreeMap<String, f32>,
}

/// One `LVLO` entry of a leveled list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeveledEntry {
    pub level: u16,
    pub form: GlobalFormId,
    pub count: u16,
}

/// An `LVLI`, `LVLN` or `LVSP` record plus what scripts added to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeveledList {
    pub chance_none: u8,
    pub flags: u8,
    pub entries: Vec<LeveledEntry>,
    /// How many entries and which chance the record shipped, what
    /// `Revert` restores.
    pub base_entries: usize,
    pub base_chance_none: u8,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LocationState {
    pub cleared: bool,
    pub keyword_data: BTreeMap<GlobalFormId, f32>,
}

/// Faction reaction codes, the `XNAM` combat values and what
/// `GetReaction` returns.
pub mod reaction {
    pub const NEUTRAL: i32 = 0;
    pub const ENEMY: i32 = 1;
    pub const ALLY: i32 = 2;
    pub const FRIEND: i32 = 3;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FactionState {
    pub reactions: BTreeMap<GlobalFormId, i32>,
    pub crime_gold_violent: i32,
    pub crime_gold_nonviolent: i32,
    pub infamy_violent: i32,
    pub infamy_nonviolent: i32,
    pub player_expelled: bool,
    pub player_enemy: bool,
    /// The `Track Crime` flag in `DATA`.
    pub tracks_crime: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActorBaseState {
    pub essential: bool,
    pub protected: bool,
    pub invulnerable: bool,
    pub unique: bool,
    pub female: bool,
    pub race: Option<GlobalFormId>,
    pub class: Option<GlobalFormId>,
    pub outfit: Option<GlobalFormId>,
    pub sleep_outfit: Option<GlobalFormId>,
    pub dead_count: i32,
}

/// A host menu request raised by a Papyrus native.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuRequest {
    /// Open the player's race and sex selection screen.
    Race,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellState {
    pub owner: Option<GlobalFormId>,
    pub public: bool,
}

/// A `RegisterForAnimationEvent` registration: the listener, the sender
/// and the event name it wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationListener {
    pub listener: ObjectId,
    pub sender: GlobalFormId,
    pub event: String,
}

/// A line-of-sight registration. `single` is a one-shot gain or lost
/// registration, `gain` says which of the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LosListener {
    pub listener: ObjectId,
    pub viewer: GlobalFormId,
    pub target: GlobalFormId,
    pub single: bool,
    pub gain: bool,
}

/// Runtime position and completed action slots for one playing scene.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct SceneRunState {
    pub phase: u32,
    pub completed_actions: BTreeSet<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectiveState {
    pub displayed: bool,
    pub completed: bool,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestState {
    pub def: Quest,
    pub running: bool,
    pub completed: bool,
    pub failed: bool,
    pub active: bool,
    /// Last stage set locally, or highest done stage after vanilla restoration.
    pub current: u16,
    /// The highest stage set, what `GetStage` returns.
    pub highest: u16,
    pub done: BTreeSet<u16>,
    pub objectives: BTreeMap<u16, ObjectiveState>,
    pub aliases: HashMap<u32, Option<GlobalFormId>>,
    pub journal: crate::quest::state::Journal,
    pub runtime: crate::quest::state::Runtime,
    /// A saved start or stop promotion task that has not completed.
    pub lifecycle_task: Option<crate::quest::state::LifecycleTask>,
}

/// Work a native asked the VM to do.
#[derive(Debug, Clone)]
pub enum Pending {
    /// A lethal request held until EndDeferredKill, including its original cause.
    DeferredDeath {
        actor: GlobalFormId,
        killer: Option<GlobalFormId>,
        silent: bool,
    },
    /// Recipients are captured when death occurs, before aliases or effects change.
    ActorDeath {
        actor: GlobalFormId,
        killer: Option<GlobalFormId>,
        recipients: Vec<Handle>,
    },
    Event {
        object: ObjectId,
        name: String,
        args: Vec<Value>,
    },
    Call {
        object: ObjectId,
        function: String,
        args: Vec<Value>,
    },
    CallScript {
        object: ObjectId,
        script: String,
        function: String,
        args: Vec<Value>,
    },
    /// Attach the winning MGEF's scripts before delivering its lifecycle events.
    BindEffect {
        object: ObjectId,
        effect: GlobalFormId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateTimer {
    pub object: ObjectId,
    pub due: f64,
    pub interval: Option<f64>,
    pub game_time: bool,
}

/// Allocates VM objects on the world's behalf. Inside a native this is
/// `NativeCall::create_object`, outside it is `Vm::create_object_of`.
pub type Alloc<'a> = &'a mut dyn FnMut(&str, &str) -> ObjectId;

pub struct World {
    pub(crate) dialogue_index: std::sync::OnceLock<crate::dialogue::Index>,
    pub(crate) dialogue: crate::dialogue::State,
    localized_strings: BTreeMap<(u16, StringTableKind), StringTable>,
    order: Arc<LoadOrder>,
    record_overrides: BTreeMap<GlobalFormId, record_overrides::RecordOverride>,
    unresolved_records:
        BTreeMap<record_overrides::UnresolvedRecordId, record_overrides::UnresolvedRecord>,
    editor_ids: HashMap<String, GlobalFormId>,
    objects: HashMap<Handle, ObjectId>,
    handles: HashMap<ObjectId, Handle>,
    refs: HashMap<GlobalFormId, RefState>,
    actors: HashMap<GlobalFormId, ActorState>,
    quests: HashMap<GlobalFormId, QuestState>,
    globals: HashMap<GlobalFormId, f32>,
    form_lists: HashMap<GlobalFormId, Vec<GlobalFormId>>,
    leveled_lists: HashMap<GlobalFormId, LeveledList>,
    actor_templates: BTreeMap<GlobalFormId, templates::TemplateChoice>,
    locations: HashMap<GlobalFormId, LocationState>,
    factions: HashMap<GlobalFormId, FactionState>,
    actor_bases: HashMap<GlobalFormId, ActorBaseState>,
    cells: HashMap<GlobalFormId, CellState>,
    encounter_levels: BTreeMap<GlobalFormId, u16>,
    encounter_zone_visits: BTreeMap<GlobalFormId, f64>,
    /// The cell whose temporary references are resident, set by a transition.
    /// See `crate::world::transition`.
    loaded_cell: Option<GlobalFormId>,
    /// Cells whose references have been built and not yet dropped, which is
    /// what decides `OnCellLoad`.
    ///
    /// Deliberately outside the determinism hash and the save. It is derived
    /// state, so the same run of calls rebuilds it identically, and vanilla
    /// does not persist its own cell cache either: the wiki says `OnCellLoad`
    /// does not fire for the first cells after loading a save, which is the
    /// behaviour an empty set gives.
    streamed_cells: std::collections::HashSet<GlobalFormId>,
    /// A cell whose detach events have fired and whose references are still
    /// resident, waiting for [`World::settle_unload`].
    ///
    /// Dropping a cell is the expensive half of leaving it, because proving a
    /// reference is unchanged means rebuilding it from its record. None of
    /// that is observable, so it does not belong on the frame the player
    /// crosses a threshold.
    pending_unload: Option<GlobalFormId>,
    next_dynamic: u32,
    player: Option<GlobalFormId>,
    /// What `PC_LEVEL_MULT` actors scale against.
    pub player_level: u32,
    rng: u64,
    pub clock: GameClock,
    pub pending: Vec<Pending>,
    /// Story Manager inputs awaiting PRG-14 node selection.
    pub story_manager_events: Vec<crate::quest::state::StoryManagerEvent>,
    pub timers: Vec<UpdateTimer>,
    pub notifications: Vec<String>,
    /// Host-facing requests raised by UI natives. These are transient commands.
    pub menu_requests: Vec<MenuRequest>,
    pub message_boxes: Vec<String>,
    pub animation_events: Vec<(GlobalFormId, String)>,
    pub player_controls: bool,
    pub ini: HashMap<String, String>,
    /// The package driver's per-world caches, see `crate::ai`.
    pub ai: crate::ai::AiWorld,
    /// What the mod data files changed, see `crate::moddata`.
    pub mod_data: crate::moddata::ModDataState,
    difficulty: crate::combat::Difficulty,
    /// `SetGameSetting*` overrides, consulted before the `GMST` record.
    pub settings: HashMap<String, Value>,
    /// Inventory event filters per listening object.
    pub inventory_filters: HashMap<ObjectId, Vec<GlobalFormId>>,
    pub sleep_listeners: Vec<ObjectId>,
    pub stats_listeners: Vec<ObjectId>,
    pub animation_listeners: Vec<AnimationListener>,
    pub los_listeners: Vec<LosListener>,
    /// Tracked stats by name, `IncrementStat` and `QueryStat`.
    pub stats: BTreeMap<String, i32>,
    pub fast_travel: bool,
    pub player_ai_driven: bool,
    pub in_chargen: bool,
    pub beast_form: bool,
    pub words_taught: BTreeSet<GlobalFormId>,
    pub words_unlocked: BTreeSet<GlobalFormId>,
    pub achievements: Vec<i32>,
    pub last_ridden_horse: Option<GlobalFormId>,
    /// `Game.AddPerkPoints`, the Dragonborn black book reward.
    pub perk_points: i32,
    /// Scenes that are playing.
    pub scenes: BTreeSet<GlobalFormId>,
    /// Phase and action completion state for playing scenes.
    pub(crate) scene_runs: BTreeMap<GlobalFormId, SceneRunState>,
    /// Sound instances by the id `Sound.Play` handed out: the sound, the
    /// source and the volume.
    pub sound_instances: BTreeMap<i32, (GlobalFormId, GlobalFormId, f32)>,
    next_sound: i32,
    /// Transient renderer requests for EffectShader Play and Stop.
    pub(crate) effect_shader_requests: Vec<events::EffectShaderRequest>,
    /// `(spell, source, target)` per `Spell.Cast`, in order.
    pub spell_casts: Vec<(GlobalFormId, GlobalFormId, Option<GlobalFormId>)>,
    pub(crate) active_effects: BTreeMap<magic::EffectId, magic::ActiveEffect>,
    pub(crate) next_effect: u64,
    /// Effect records and magnitudes for potions created by alchemy.
    pub(crate) brewed_potions: BTreeMap<GlobalFormId, Vec<(GlobalFormId, f32, u32, u32)>>,
    pub(crate) concentration_cast: Option<magic::ConcentrationCast>,
    /// A transient reference held by one Telekinesis concentration cast.
    /// Physics handles are host-owned, so this state is never serialized.
    pub(crate) telekinesis_grab: Option<(GlobalFormId, GlobalFormId)>,
    pub(crate) image_space: image_space::ImageSpaceState,
    /// Stealth-point pools by target, each with its observers' awareness.
    pub(crate) detection: crate::detection::DetectionState,
    pub(crate) story_history: BTreeMap<GlobalFormId, (u32, f64)>,
    pub(crate) story_selection: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    /// Detection sounds awaiting one host observation step.
    pub(crate) detection_events: Vec<crate::detection::DetectionEvent>,
}

impl World {
    pub fn new(order: LoadOrder) -> World {
        Self::from_shared_order(std::sync::Arc::new(order))
    }

    /// Use the immutable form table already opened by the host.
    pub fn from_shared_order(order: Arc<LoadOrder>) -> World {
        let mut editor_ids = HashMap::new();
        for (file_index, file) in order.files().iter().enumerate() {
            for (i, rec) in file.plugin.records.iter().enumerate() {
                let id = order.resolve(file_index, rec.form_id);
                let is_winner = order.winner_ref(id)
                    == Some(RecordRef {
                        file: file_index,
                        record: i,
                    });
                if !is_winner {
                    continue;
                }
                if let Some(edid) = rec.editor_id() {
                    editor_ids.insert(edid.to_ascii_lowercase(), id);
                }
            }
        }
        World {
            order,
            record_overrides: BTreeMap::new(),
            unresolved_records: BTreeMap::new(),
            editor_ids,
            objects: HashMap::new(),
            handles: HashMap::new(),
            refs: HashMap::new(),
            actors: HashMap::new(),
            quests: HashMap::new(),
            globals: HashMap::new(),
            form_lists: HashMap::new(),
            leveled_lists: HashMap::new(),
            actor_templates: BTreeMap::new(),
            locations: HashMap::new(),
            factions: HashMap::new(),
            actor_bases: HashMap::new(),
            cells: HashMap::new(),
            encounter_levels: BTreeMap::new(),
            encounter_zone_visits: BTreeMap::new(),
            loaded_cell: None,
            streamed_cells: std::collections::HashSet::new(),
            pending_unload: None,
            next_dynamic: 0x1000,
            player: None,
            player_level: 1,
            rng: 0x9E37_79B9_7F4A_7C15,
            clock: GameClock::default(),
            pending: Vec::new(),
            story_manager_events: Vec::new(),
            timers: Vec::new(),
            notifications: Vec::new(),
            menu_requests: Vec::new(),
            message_boxes: Vec::new(),
            animation_events: Vec::new(),
            player_controls: true,
            ini: HashMap::new(),
            ai: crate::ai::AiWorld::default(),
            mod_data: crate::moddata::ModDataState::default(),
            difficulty: crate::combat::Difficulty::default(),
            settings: HashMap::new(),
            inventory_filters: HashMap::new(),
            sleep_listeners: Vec::new(),
            stats_listeners: Vec::new(),
            animation_listeners: Vec::new(),
            los_listeners: Vec::new(),
            stats: BTreeMap::new(),
            fast_travel: true,
            player_ai_driven: false,
            in_chargen: false,
            beast_form: false,
            words_taught: BTreeSet::new(),
            words_unlocked: BTreeSet::new(),
            achievements: Vec::new(),
            last_ridden_horse: None,
            perk_points: 0,
            scenes: BTreeSet::new(),
            scene_runs: BTreeMap::new(),
            sound_instances: BTreeMap::new(),
            next_sound: 0,
            effect_shader_requests: Vec::new(),
            spell_casts: Vec::new(),
            dialogue_index: std::sync::OnceLock::new(),
            dialogue: crate::dialogue::State::default(),
            localized_strings: BTreeMap::new(),
            active_effects: BTreeMap::new(),
            next_effect: 1,
            brewed_potions: BTreeMap::new(),
            concentration_cast: None,
            telekinesis_grab: None,
            image_space: image_space::ImageSpaceState::default(),
            detection: crate::detection::DetectionState::default(),
            story_history: BTreeMap::new(),
            story_selection: BTreeMap::new(),
            detection_events: Vec::new(),
        }
    }

    pub fn order(&self) -> &LoadOrder {
        &self.order
    }
}

fn le_u32(d: &[u8]) -> Option<u32> {
    (d.len() >= 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
}

fn le_f32(d: &[u8]) -> f32 {
    f32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

fn text(d: &[u8], localized: bool) -> Text {
    match (localized, le_u32(d)) {
        (true, Some(n)) if d.len() == 4 => Text::Localized(n),
        _ => Text::Plain(soe_esm::cursor::zstring(d)),
    }
}

pub fn class_for_tag(tag: &[u8; 4]) -> &'static str {
    match tag {
        b"REFR" => "ObjectReference",
        b"ACHR" => "Actor",
        b"QUST" => "Quest",
        b"CELL" => "Cell",
        b"WRLD" => "WorldSpace",
        b"GLOB" => "GlobalVariable",
        b"KYWD" => "Keyword",
        b"NPC_" => "ActorBase",
        b"ACTI" => "Activator",
        b"DOOR" => "Door",
        b"CONT" => "Container",
        b"STAT" => "Static",
        b"MISC" => "MiscObject",
        b"WEAP" => "Weapon",
        b"ARMO" => "Armor",
        b"BOOK" => "Book",
        b"ALCH" => "Potion",
        b"INGR" => "Ingredient",
        b"KEYM" => "Key",
        b"FLST" => "FormList",
        b"FACT" => "Faction",
        b"SPEL" => "Spell",
        b"LCTN" => "Location",
        b"MGEF" => "MagicEffect",
        b"FURN" => "Furniture",
        b"LIGH" => "Light",
        b"MESG" => "Message",
        b"RACE" => "Race",
        b"PACK" => "Package",
        b"SCEN" => "Scene",
        b"SNDR" => "Sound",
        b"IDLE" => "Idle",
        b"PERK" => "Perk",
        b"SHOU" => "Shout",
        b"WOOP" => "WordOfPower",
        b"ENCH" => "Enchantment",
        b"AMMO" => "Ammo",
        b"LVLI" => "LeveledItem",
        b"LVLN" => "LeveledActor",
        b"LVSP" => "LeveledSpell",
        b"IMAD" => "ImageSpaceModifier",
        b"WTHR" => "Weather",
        b"TXST" => "TextureSet",
        b"OTFT" => "Outfit",
        b"CLAS" => "Class",
        b"CSTY" => "CombatStyle",
        b"VTYP" => "VoiceType",
        b"MUSC" => "MusicType",
        b"SLGM" => "SoulGem",
        b"SCRL" => "Scroll",
        b"APPA" => "Apparatus",
        b"FLOR" => "Flora",
        b"TACT" => "TalkingActivator",
        b"EXPL" => "Explosion",
        b"PROJ" => "Projectile",
        b"HAZD" => "Hazard",
        b"EFSH" => "EffectShader",
        b"RFCT" => "VisualEffect",
        b"SPGD" => "ShaderParticleGeometry",
        b"ECZN" => "EncounterZone",
        b"DIAL" => "Topic",
        b"INFO" => "TopicInfo",
        b"ARTO" => "Art",
        b"IPDS" => "ImpactDataSet",
        b"HDPT" => "HeadPart",
        b"CLFM" => "ColorForm",
        b"ASTP" => "AssociationType",
        b"LCRT" => "LocationRefType",
        b"COBJ" => "ConstructibleObject",
        _ => "Form",
    }
}

pub fn type_code_for_tag(tag: &[u8; 4]) -> i32 {
    match tag {
        b"KYWD" => 4,
        b"FACT" => 8,
        b"GLOB" => 9,
        b"CLAS" => 10,
        b"RACE" => 14,
        b"SPEL" => 22,
        b"ACTI" => 24,
        b"ARMO" => 26,
        b"BOOK" => 27,
        b"CONT" => 28,
        b"DOOR" => 29,
        b"INGR" => 30,
        b"LIGH" => 31,
        b"MISC" => 32,
        b"STAT" => 34,
        b"FURN" => 40,
        b"WEAP" => 41,
        b"AMMO" => 42,
        b"NPC_" => 43,
        b"KEYM" => 45,
        b"ALCH" => 46,
        b"WRLD" => 59,
        b"CELL" => 60,
        b"REFR" => 61,
        b"ACHR" => 62,
        b"QUST" => 77,
        b"FLST" => 91,
        b"LCTN" => 104,
        _ => 0,
    }
}
