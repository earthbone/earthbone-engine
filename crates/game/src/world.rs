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
//! - `training`: trainer eligibility, costs and saved use limits.
//! - `rng`: the random number stream.
//! - `driver`: what the package driver reads and calls.
//! - `save`: the `SOEW` save format.
//! - `hash`: the determinism hash.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use soe_commonlib_shim::events::{ActorKill, CriticalHit, EventFormId, EventSource};
use soe_esm::records::{Text, Transform};
use soe_esm::strings::{StringTable, StringTableKind};
use soe_esm::{GlobalFormId, LoadOrder, RecordRef, Tag};
use soe_papyrus::vm::LatentId;
use soe_papyrus::{ObjectId, Value};

use crate::clock::GameClock;
use crate::quest::Quest;

mod actor_seeds;
mod actor_spells;
mod actors;
mod base_object_restore;
mod bleedout;
mod calendar;
mod combat;
pub mod commonlib;
mod commonlib_events;
pub mod commonlib_magic;
mod contact;
mod crime;
mod death;
mod destruction;
mod detection;
mod dialogue_scene_restore;
mod doorway;
mod driver;
pub mod effect_shader;
mod encounter_zones;
mod events;
pub use events::EffectShaderRequest;
pub use events::ReferencePhysicsRequest;
mod forms;
mod furniture;
mod game_ui;
pub use game_ui::GameUiError;
pub mod hash;
mod inventory;
mod inventory_restore;
mod item_restore;
mod knockback;
mod spell_hand;
mod weapon_hands;
pub mod weather;
pub use inventory_restore::InventoryRestoreReport;
mod activator_restore;
mod book_note_key_restore;
pub mod image_space;
mod item_save;
mod list_restore;
mod localization;
mod locks;
mod lookup;
pub mod magic;
mod music;
mod npc_restore;
mod objects;
mod outfits;
mod package_navmesh_restore;
mod performance;
mod projectile_restore;
pub mod shader_particles;
pub use npc_restore::NpcRestoreReport;
mod record_change_restore;
mod record_overrides;
mod skins;
mod temporary_npc;
pub use record_overrides::{
    ActivatorRecordExtra, BaseObjectRecordExtra, BookNoteKeyRecordExtra, ItemRecordExtra,
    NpcRecordExtra, ProjectileRecordExtra, StaticObjectRecordExtra,
};
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
mod static_object_restore;
pub use quest_restore::QuestRestoreReport;
mod light_references;
mod reference_extras;
mod reference_restore;
mod refs;
mod shim_extras;
mod spatial_restore;
pub use reference_restore::{MissingPlacementCell, ReferenceRemainder, ReferenceRestoreReport};
pub use refs::RefPlacement;
mod leveled_items;
mod rng;
pub use rng::RngStream;
pub(crate) mod save;
mod save_section_registry;
mod save_sections;
mod scr_40;
mod shim;
mod shim_runtime;
mod template_restore;
mod templates;
pub use template_restore::TemplateRestoreReport;
mod reference_translation;
mod timers;
pub mod training;
mod transition;
mod trigger;
mod visual_effects;
pub use reference_translation::{RefTranslation, TranslationPath};

pub use contact::{ContactActor, DEFAULT_COMBAT_DISTANCE};
pub use dialogue_scene_restore::DialogueSceneRestoreReport;
pub use doorway::{Doorway, InteriorDestination, Side, Threshold, THRESHOLD_BAND, UNITS_PER_METRE};
pub use projectiles::RangedEquipment;
pub use reference_extras::{ReferenceExtrasRestoreReport, ReferenceLock};
pub use save::{SAVE_MAGIC, SAVE_VERSION};
pub use transition::CellTransition;
pub use visual_effects::{VisualEffectAttachment, VisualEffectData};
pub use visual_effects::{VisualEffectError, VisualEffectId, VisualEffectInstance};
pub use weather::WeatherRequest;

/// Created references and temporary bases share an internal namespace outside
/// the plugin positions. Runtime FormIDs still use the `FF` prefix.
pub const DYNAMIC_FILE: u16 = soe_esm::CREATED_FILE;
pub const PLAYER_OBJECT: u32 = 0x14;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// Transient camera controls requested by Game natives for the host player.
/// These affect presentation and are not part of the saved world state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraControls {
    pub target: Option<GlobalFormId>,
    pub first_person_geometry: bool,
    /// Degrees offset from the normal camera angle while sitting.
    pub sitting_rotation_degrees: f32,
}

impl Default for CameraControls {
    fn default() -> Self {
        Self {
            target: None,
            first_person_geometry: true,
            sitting_rotation_degrees: 0.0,
        }
    }
}

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

/// A host action requested by a Papyrus Debug native.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugHostRequest {
    Quit,
    CenterOnCell {
        cell: GlobalFormId,
        latent: Option<LatentId>,
    },
    MovePlayerToReference {
        reference: GlobalFormId,
        latent: LatentId,
    },
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
    /// A door animation in progress. The settled destination is in `opened`.
    pub open_transition: Option<DoorOpenTransition>,
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
    /// Remaining health and active DEST stage for this placed object.
    pub destruction_health: Option<f32>,
    pub destruction_stage: i32,
    /// Set by `ObjectReference.SetNoFavorAllowed` for follower interactions.
    pub no_favor_allowed: bool,
    /// Cleared destruction effects do not return on subsequent damage.
    pub destruction_cleared: bool,
    pub ignore_friendly_hits: bool,
    /// Animation graph variables set through `SetAnimationVariable*`, keyed
    /// by lowercase name. The graph has one value per name, the three
    /// typed pairs read and write the same store.
    pub anim_floats: BTreeMap<String, f32>,
    pub anim_ints: BTreeMap<String, i32>,
    pub anim_bools: BTreeMap<String, bool>,
    /// `SetAlpha`, the fade the Dawnguard and Dragonborn scripts use.
    pub alpha: f32,
    /// An in-flight `TranslateTo` or `SplineTranslateTo` action.
    pub translation: Option<reference_translation::RefTranslation>,
}

/// The transitional state of an animated door reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorOpenTransition {
    Opening,
    Closing,
}

impl RefState {
    pub(crate) fn open_state(&self, is_door: bool) -> i32 {
        if !is_door {
            return 0;
        }
        match self.open_transition {
            Some(DoorOpenTransition::Opening) => 2,
            Some(DoorOpenTransition::Closing) => 4,
            None if self.opened => 1,
            None => 3,
        }
    }

    pub(crate) fn new(base: Option<GlobalFormId>) -> Self {
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
            open_transition: None,
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
            destruction_health: None,
            destruction_stage: -1,
            no_favor_allowed: false,
            destruction_cleared: false,
            ignore_friendly_hits: false,
            anim_floats: BTreeMap::new(),
            anim_ints: BTreeMap::new(),
            anim_bools: BTreeMap::new(),
            alpha: 1.0,
            translation: None,
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
    pub can_do_favors: bool,
    pub ghost: bool,
    pub essential: bool,
    pub protected: bool,
    pub level: u32,
    /// Character experience banked toward the next level.
    pub character_experience: f32,
    /// Unspent attribute choices earned from character level-ups.
    pub level_up_choices: u32,
    pub race: Option<GlobalFormId>,
    pub unconscious: bool,
    /// Explicit `ForceAddRagdollToWorld`/`ForceRemoveRagdollFromWorld` override.
    /// `None` follows death or unconscious state.
    pub ragdoll_forced: Option<bool>,
    pub doing_favor: bool,
    pub factions: HashMap<GlobalFormId, i32>,
    pub spells: BTreeSet<GlobalFormId>,
    /// Forms the actor has marked in the magic favorites list.
    pub favorites: BTreeSet<GlobalFormId>,
    /// CK knowable forms whose `Known` flag is set for the player.
    pub known_forms: BTreeSet<GlobalFormId>,
    pub equipped: BTreeSet<GlobalFormId>,
    /// Explicit weapon slots. The equipment set retains armor and spell membership.
    pub left_weapon: Option<GlobalFormId>,
    pub right_weapon: Option<GlobalFormId>,
    /// Explicit casting-hand spell slots, set through `World::equip_hand_spell`.
    /// Independent of `left_weapon`/`right_weapon`, both a weapon and a spell
    /// slot may hold a value for the same hand at once.
    pub left_spell: Option<GlobalFormId>,
    pub right_spell: Option<GlobalFormId>,
    /// Explicit spell source slots for voice and instant casting.
    pub other_spell: Option<GlobalFormId>,
    pub instant_spell: Option<GlobalFormId>,
    /// The package driver's state, see `crate::ai`.
    pub ai: crate::ai::AiState,
    /// Papyrus KeepOffsetFromActor movement request.
    pub keep_offset: Option<ActorKeepOffset>,
    /// Reference forced as this actor's landing marker.
    pub forced_landing_marker: Option<GlobalFormId>,
    /// SPID packages and their zero-based insertion positions in the base
    /// package list. Persisted with the actor because distribution is not
    /// rerun when SOEW restores an existing actor.
    pub package_overrides: Vec<(usize, GlobalFormId)>,
    pub perks: BTreeSet<GlobalFormId>,
    pub alerted: bool,
    pub sneaking: bool,
    /// The actor's current swimming state, supplied by runtime movement.
    pub swimming: bool,
    /// The actor's current running state, supplied by runtime movement.
    pub running: bool,
    /// Whether the actor is currently riding a mount.
    pub riding_mount: bool,
    /// Whether another actor is currently riding this actor.
    pub being_ridden: bool,
    pub weapon_drawn: bool,
    /// The host movement driver's current sprint state, re-signaled after load.
    pub sprinting: bool,
    /// Whether the live movement driver currently has the actor in flight.
    /// This is transient and must be signaled from runtime movement.
    pub flying: bool,
    /// The host animation graph's current block stance. Unlike the other
    /// combat stance flags above, this is not part of saved state: the host
    /// re-signals it from the live graph after load, through
    /// `World::set_blocking`.
    pub blocking: bool,
    /// Ward animation state supplied by the host. `None` means the host has
    /// not reported a state for this actor in the current session.
    pub ward_state: Option<u32>,
    /// Whether the host animation graph reports an active dual cast.
    pub dual_casting: Option<bool>,
    /// Active shout voice variation reported by the host animation graph.
    pub shout_variation: Option<i32>,
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
    /// Whether this actor is currently trespassing, for the CK `IsTrespassing` query.
    pub trespassing: bool,
    pub outfit: Option<GlobalFormId>,
    pub sleep_outfit: Option<GlobalFormId>,
    pub killer: Option<GlobalFormId>,
    /// Behavior graph floats set through `SetSubGraphFloatVariable`, keyed
    /// by lowercase name.
    pub graph_floats: BTreeMap<String, f32>,
    /// Integer values reported by the live behavior graph, keyed by lowercase name.
    pub graph_ints: BTreeMap<String, i32>,
    /// `SetAllowFlying`, the Dawnguard dragon riding flag.
    pub allow_flying: bool,
    /// Live flying state reported by actor movement, using CK states 0 to 4.
    pub flying_state: Option<u8>,
    /// Between `StartDeferredKill` and `EndDeferredKill` death waits.
    pub deferred_kill: bool,
    /// The furniture reference and zero-based marker currently reserved by
    /// this actor. The marker remains reserved until the actor stands.
    pub furniture_seat: Option<(GlobalFormId, u16)>,
    /// The last state `SendVampirismStateChanged` announced for this actor.
    pub vampire: bool,
    /// The last state `SendLycanthropyStateChanged` announced.
    pub werewolf: bool,
    /// Race and race effects owned by one active Werewolf archetype effect.
    pub(crate) werewolf_transform: Option<WerewolfTransformState>,
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
    /// Real-time deadline before this actor may shout again.
    pub voice_recovery_until: Option<f64>,
    /// Real-time deadline for the stagger animation and input lock a melee
    /// hit above `fStaggerThreshold` sets, `None` when not staggered.
    /// Compared against `World::clock.real_seconds`, the lazy expiry
    /// `ai::AiState::wait_until` also uses, see `World::is_staggered`.
    pub stagger_end_time: Option<f64>,
    /// Magnitude passed to the shared stagger lifecycle by the latest hit or
    /// magic effect.
    pub stagger_magnitude: f64,
    /// Form that supplied the latest stagger direction.
    pub stagger_direction: Option<GlobalFormId>,
    /// Perpetrators this actor has witnessed committing a crime against a
    /// faction that tracks crime. The bounty itself lives on
    /// `FactionState`, this is the actor's own memory of who it saw do it.
    /// See `World::record_witnessed_crime`.
    pub witnessed_crimes: BTreeSet<GlobalFormId>,
    /// Papyrus expression override, retained until `ClearExpressionOverride`.
    pub expression_override: Option<ExpressionOverride>,
    /// Partial use experience for each actor skill, keyed by lowercase name.
    pub skill_experience: BTreeMap<String, f32>,
    /// Cumulative in-game days served in prison.
    pub days_in_jail: u32,
    /// Crime faction whose jail property chest holds the player's belongings.
    pub jail_faction: Option<GlobalFormId>,
    /// Explicit `SetHeadTracking` value. `None` keeps the engine default enabled.
    pub head_tracking: Option<bool>,
    /// Reference attached through Papyrus `Actor.SetVehicle`.
    pub vehicle: Option<GlobalFormId>,
    /// Script-forced look target and whether the actor paths toward it.
    pub forced_look_at: Option<(GlobalFormId, bool)>,
}

/// Papyrus request for an actor to maintain a body-space offset from a target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorKeepOffset {
    /// Actor whose body space defines the offset.
    pub target: GlobalFormId,
    /// X, Y and Z body-space offsets.
    pub offset: [f32; 3],
    /// X, Y and Z angle offsets in degrees.
    pub angles: [f32; 3],
    /// Distance beyond which the actor catches up at run speed.
    pub catch_up_radius: f32,
    /// Distance within which the actor stays still.
    pub follow_radius: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WerewolfTransformState {
    pub effect: crate::world::magic::EffectId,
    pub prior_race: Option<GlobalFormId>,
    pub transformed_race: GlobalFormId,
    pub added_spells: BTreeSet<GlobalFormId>,
    pub added_shouts: BTreeSet<GlobalFormId>,
    pub added_effects: BTreeSet<crate::world::magic::EffectId>,
}

/// A CK facial expression id and its 0 to 100 strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressionOverride {
    /// CK `aiMood` value, from 0 through 15.
    pub mood: u8,
    /// Expression strength in percent.
    pub strength: u8,
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
    /// Whether the player has committed a melee hit against this faction.
    pub player_attacked: bool,
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
    /// Load race menu head part models and morph data before opening it.
    PrecacheCharGen,
    /// Release data retained by `PrecacheCharGen`.
    PrecacheCharGenClear,
    /// Open the player's race and sex selection screen.
    Race,
    /// Open the training menu for the named actor.
    Training(GlobalFormId),
    /// Show the named actor's barter service in the host UI.
    Barter(GlobalFormId),
    /// Show the named actor's inventory in the host UI.
    ActorInventory(GlobalFormId),
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

/// A latent `WaitForAnimationEvent` call waiting on one reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationWaiter {
    pub source: GlobalFormId,
    pub event: String,
    pub latent: LatentId,
}

/// A named event to send to one placed reference's animation graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationRequest {
    pub target: GlobalFormId,
    pub animation: String,
}

/// Result waiting for the game loop to resolve in the Papyrus VM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationCompletion {
    pub latent: LatentId,
    pub succeeded: bool,
}

/// One ordered notification from a host animation visual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationHostNotice {
    Event { source: GlobalFormId, name: String },
    Unloaded { target: GlobalFormId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnimationWait {
    pub target: GlobalFormId,
    pub event: String,
    pub latent: LatentId,
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
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SceneRunState {
    pub phase: u32,
    pub completed_actions: BTreeSet<u32>,
    pub started_actions: BTreeSet<u32>,
    pub timer_deadlines: BTreeMap<u32, f64>,
    pub prior_packages: BTreeMap<GlobalFormId, Option<GlobalFormId>>,
    pub prior_aliases: BTreeMap<GlobalFormId, Option<(GlobalFormId, u32)>>,
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
    /// A cell event whose reference still needs a script binding in Game.
    ReferenceEvent {
        reference: GlobalFormId,
        name: String,
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
    /// Attach the winning base object's VMAD scripts to a placed reference.
    BindReference {
        object: ObjectId,
        base: GlobalFormId,
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

/// One-shot request for a host-owned screen or presentation action.
#[derive(Debug, Clone, PartialEq)]
pub enum HostRequest {
    /// Tear down gameplay and return to the main menu.
    QuitToMainMenu,
    /// Start the named title sequence on the main menu.
    StartTitleSequence(String),
    FadeOutGame {
        fading_out: bool,
        black: bool,
        seconds_before_fade: f32,
        fade_duration: f32,
    },
    Save {
        automatic: bool,
    },
    CameraFirstPerson,
    CameraThirdPerson,
    ControllerShake {
        left_strength: f32,
        right_strength: f32,
        duration: f32,
    },
}

/// Operation requested by Papyrus to load or release a spell's art resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellArtAction {
    /// Load art referenced by the spell for later use.
    Preload,
    /// Release art previously requested by `Preload`.
    Unload,
}

/// One spell art operation awaiting the host resource provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellArtRequest {
    /// Spell whose art resources are requested.
    pub spell: GlobalFormId,
    /// Requested resource operation.
    pub action: SpellArtAction,
}

pub struct World {
    /// Runtime plugin listeners. These are host state, not game save data.
    pub(crate) actor_kill_events: EventSource<ActorKill>,
    pub(crate) critical_hit_events: EventSource<CriticalHit>,
    pub(crate) dialogue_index: std::sync::OnceLock<crate::dialogue::Index>,
    quest_alias_candidates: std::sync::OnceLock<Option<Arc<[GlobalFormId]>>>,
    pub(crate) dialogue: crate::dialogue::State,
    pub(crate) known_dialogue_topics: BTreeSet<GlobalFormId>,
    /// NPC bases warned about during this world session, excluded from saves.
    pub(crate) duplicate_npc_perk_warnings: BTreeSet<GlobalFormId>,
    /// Source-level CommonLib event channels backed by this live world.
    pub(crate) commonlib_events: soe_commonlib_shim::events::ScriptEventSourceHolder,
    localized_strings: BTreeMap<(u16, StringTableKind), StringTable>,
    order: Arc<LoadOrder>,
    record_overrides: BTreeMap<GlobalFormId, record_overrides::RecordOverride>,
    spatial_change_payloads: BTreeMap<GlobalFormId, Vec<u8>>,
    created_base_records: BTreeMap<GlobalFormId, record_overrides::RecordOverride>,
    unresolved_records:
        BTreeMap<record_overrides::UnresolvedRecordId, record_overrides::UnresolvedRecord>,
    editor_ids: HashMap<String, GlobalFormId>,
    objects: HashMap<Handle, ObjectId>,
    handles: HashMap<ObjectId, Handle>,
    refs: HashMap<GlobalFormId, RefState>,
    light_references: std::cell::RefCell<light_references::LightReferences>,
    /// Current occupants, derived from actor movement and XPRM placement.
    trigger_occupants: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    trigger_index: Option<BTreeMap<GlobalFormId, Vec<GlobalFormId>>>,
    actors: HashMap<GlobalFormId, ActorState>,
    pub(crate) follower_services: BTreeMap<GlobalFormId, crate::services::FollowerServiceState>,
    quests: HashMap<GlobalFormId, QuestState>,
    quest_start_stop_events:
        soe_commonlib_shim::events::EventSource<soe_commonlib_shim::events::QuestStartStopEvent>,
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
    /// Instructions executed by the most recent Papyrus scheduler slice.
    pub(crate) papyrus_instructions_last_slice: usize,
    /// Exterior cells loaded by the most recent host streaming update.
    pub(crate) exterior_cell_loads_last_update: usize,
    /// References whose host cell has loaded 3D. Rebuilt from cell streaming
    /// transitions and not saved, like `streamed_cells`.
    loaded_3d_references: std::collections::HashSet<GlobalFormId>,
    /// Cells attached to the runtime world, distinct from prefetched cells.
    attached_cells: std::collections::HashSet<GlobalFormId>,
    /// Cells marked by `Cell.Reset`, consumed the next time they are loaded.
    pub(crate) pending_cell_resets: std::collections::BTreeSet<GlobalFormId>,
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
    /// Host interaction results for the current rendered frame.
    crosshair_reference: Option<GlobalFormId>,
    grabbed_reference: Option<GlobalFormId>,
    player_first_person: bool,
    /// Save character ID used by deterministic SPID distribution rules.
    player_character_id: u64,
    /// What `PC_LEVEL_MULT` actors scale against.
    pub player_level: u32,
    /// Player training purchases used at the current character level.
    pub(crate) training_sessions_this_level: u32,
    pub(crate) training_session_level: u32,
    pub(crate) pending_training_menu: Option<GlobalFormId>,
    rng: u64,
    rng_streams: [u64; rng::STREAM_COUNT],
    pub clock: GameClock,
    pub(crate) menu_mode: bool,
    /// Actor pairs visible during the latest successful host perception step.
    pub(crate) line_of_sight: BTreeSet<(GlobalFormId, GlobalFormId)>,
    pub(crate) frame_rate: performance::FrameRateState,
    pub pending: Vec<Pending>,
    /// Transient requests for work that only the host can present.
    pub host_requests: Vec<HostRequest>,
    pub(crate) spell_art_requests: Vec<SpellArtRequest>,
    pub(crate) preloaded_spell_art: BTreeSet<GlobalFormId>,
    /// Story Manager inputs awaiting PRG-14 node selection.
    pub story_manager_events: Vec<crate::quest::state::StoryManagerEvent>,
    /// Script events submitted by Papyrus and waiting for Story Manager routing.
    pub(crate) pending_story_events: Vec<crate::quest::state::StoryEvent>,
    pub timers: Vec<UpdateTimer>,
    pub notifications: Vec<String>,
    /// Recoverable errors raised by void Papyrus Game natives.
    pub native_errors: Vec<(String, String)>,
    /// Host-facing requests raised by UI natives. These are transient commands.
    pub menu_requests: Vec<MenuRequest>,
    pub message_boxes: Vec<String>,
    /// Screenshot paths requested by Papyrus for the host to capture.
    pub screenshot_requests: Vec<String>,
    help_message_events: BTreeSet<String>,
    pub animation_events: Vec<(GlobalFormId, String)>,
    /// One-shot impulses waiting for the host's ragdoll bodies.
    pub actor_impulses: Vec<ActorImpulse>,
    /// Requests awaiting delivery to a host animation graph.
    pub animation_requests: Vec<AnimationRequest>,
    pub(crate) animation_waits: Vec<AnimationWait>,
    /// Ordered events and unloads reported by host animation visuals.
    pub animation_host_notices: Vec<AnimationHostNotice>,
    /// Completed latent calls awaiting the game loop.
    pub animation_completions: Vec<AnimationCompletion>,
    /// Open Papyrus user logs and the lines written to each during this run.
    pub user_logs: BTreeMap<String, Vec<String>>,
    /// User log writes awaiting delivery to the file logger.
    pub(crate) pending_user_log_writes: Vec<(String, String)>,
    /// One-shot requests for the host, which owns process and cell streaming.
    pub debug_host_requests: Vec<DebugHostRequest>,
    /// Enabled states for movement, fighting, camera, looking, sneaking, menu,
    /// activation and journal controls, in that order.
    pub player_control_flags: [bool; 8],
    /// POV system that last changed the player's camera-switch control.
    pub player_control_pov_type: i32,
    pub player_controls: bool,
    /// Camera state requested by Papyrus and consumed by the host.
    pub camera_controls: CameraControls,
    /// Runtime switches changed by the Papyrus `Debug` globals.
    pub debug_god_mode: bool,
    pub debug_ai_enabled: bool,
    pub debug_collisions_enabled: bool,
    /// The state requested by `Debug.SetFootIK` for host actor presentation.
    pub debug_foot_ik: bool,
    /// References requested by `Debug.ShowRefPosition` for development markers.
    pub(crate) debug_ref_positions: BTreeSet<GlobalFormId>,
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
    /// The active Papyrus gift menu request, if one is waiting for the host.
    pub gift_menu: Option<GiftMenuRequest>,
    pub sleep_listeners: Vec<ObjectId>,
    pub stats_listeners: Vec<ObjectId>,
    pub animation_listeners: Vec<AnimationListener>,
    /// Papyrus waits keyed by their reference and case-insensitive event name.
    pub animation_waiters: Vec<AnimationWaiter>,
    /// Results ready for `Game::pump` to deliver to the VM.
    pub completed_animation_waits: Vec<(LatentId, bool)>,
    pub los_listeners: Vec<LosListener>,
    /// Tracked stats by name, `IncrementStat` and `QueryStat`.
    pub stats: BTreeMap<String, i32>,
    pub fast_travel: bool,
    pub player_ai_driven: bool,
    pub in_chargen: bool,
    /// Whether chargen disables saving.
    pub chargen_disable_saving: bool,
    /// Whether chargen should block waiting and show a controls-disabled HUD message.
    pub chargen_disable_waiting: bool,
    pub chargen_show_controls_disabled_message: bool,
    /// `Game.SetHudCartMode` state for the host HUD presenter.
    pub hud_cart_mode: bool,
    /// Transient presentation requests raised by global Game natives.
    pub game_ui_requests: Vec<GameUiRequest>,
    pub beast_form: bool,
    /// Host camera direction in Skyrim axes, refreshed by the player adapter.
    pub camera_look: [f32; 3],
    /// One-shot camera effects requested by Papyrus and consumed by the host.
    pub camera_shakes: Vec<CameraShake>,
    pub words_taught: BTreeSet<GlobalFormId>,
    pub words_unlocked: BTreeSet<GlobalFormId>,
    /// Magic effects made available for new enchantments by disenchanting.
    pub learned_enchantment_effects: BTreeSet<GlobalFormId>,
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
    /// Transient host physics operations requested by Papyrus natives.
    pub reference_physics_requests: Vec<events::ReferencePhysicsRequest>,
    /// Active `MusicType.Add` entries in the order scripts added them.
    music_types: Vec<GlobalFormId>,
    /// `(spell, source, target)` per `Spell.Cast`, in order.
    pub spell_casts: Vec<(GlobalFormId, GlobalFormId, Option<GlobalFormId>)>,
    pub(crate) active_effects: BTreeMap<magic::EffectId, magic::ActiveEffect>,
    /// Visual effects currently attached to references. Host rendering is
    /// transient and is rebuilt from Papyrus calls after load.
    visual_effect_attachments: BTreeMap<(GlobalFormId, GlobalFormId), VisualEffectAttachment>,
    pub(crate) cloak_effects: BTreeMap<magic::EffectId, magic::CloakState>,
    pub(crate) next_effect: u64,
    pub(crate) spell_casts_in_progress: BTreeMap<GlobalFormId, magic::SpellCast>,
    pub(crate) vampire_lord_forms: BTreeMap<magic::EffectId, magic::VampireLordForm>,
    /// Effect records and magnitudes for potions created by alchemy.
    pub(crate) brewed_potions: BTreeMap<GlobalFormId, Vec<(GlobalFormId, f32, u32, u32)>>,
    pub(crate) concentration_cast: Option<magic::ConcentrationCast>,
    /// A transient reference held by one Telekinesis concentration cast.
    /// Physics handles are host-owned, so this state is never serialized.
    pub(crate) telekinesis_grab: Option<(GlobalFormId, GlobalFormId)>,
    pub(crate) visual_effects:
        BTreeMap<visual_effects::VisualEffectId, visual_effects::VisualEffectInstance>,
    pub(crate) next_visual_effect: u64,
    pub(crate) image_space: image_space::ImageSpaceState,
    pub(crate) sun_gaze_modifier: Option<GlobalFormId>,
    pub(crate) sun_gazing_last_tick: bool,
    pub(crate) weather_image_space: Option<image_space::WeatherSelection>,
    /// Active shader particle geometries derived from winning SPGD records.
    pub(crate) shader_particles: shader_particles::ShaderParticleState,
    pub(crate) effect_shaders: effect_shader::EffectShaderState,
    /// Stealth-point pools by target, each with its observers' awareness.
    pub(crate) detection: crate::detection::DetectionState,
    pub(crate) story_history: BTreeMap<GlobalFormId, (u32, f64)>,
    pub(crate) story_selection: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    /// Detection sounds awaiting one host observation step.
    pub(crate) detection_events: Vec<crate::detection::DetectionEvent>,
    pub(crate) weather: weather::WeatherState,
}

impl soe_commonlib_shim::world::ActorValueWorldState for World {
    fn actor_value(&self, actor: u32, value: soe_commonlib_shim::world::ActorValue) -> Option<f32> {
        let id = self
            .actors
            .keys()
            .copied()
            .find(|id| self.order.runtime_id(*id) == actor)?;
        let name = crate::actor_values::actor_value_name(i32::try_from(value.0).ok()?)?;
        Some(self.actors.get(&id)?.values.get(name).current())
    }

    fn set_actor_value(
        &mut self,
        actor: u32,
        value: soe_commonlib_shim::world::ActorValue,
        amount: f32,
    ) -> bool {
        let Some(id) = self
            .actors
            .keys()
            .copied()
            .find(|id| self.order.runtime_id(*id) == actor)
        else {
            return false;
        };
        let Some(name) = i32::try_from(value.0)
            .ok()
            .and_then(crate::actor_values::actor_value_name)
        else {
            return false;
        };
        self.set_actor_value(id, name, amount).is_ok()
    }

    fn actor_value_name(&self, value: soe_commonlib_shim::world::ActorValue) -> Option<String> {
        crate::actor_values::actor_value_name(i32::try_from(value.0).ok()?).map(str::to_string)
    }

    fn actor_value_by_name(&self, name: &str) -> Option<soe_commonlib_shim::world::ActorValue> {
        crate::actor_values::actor_value_index(name)
            .and_then(|index| u32::try_from(index).ok())
            .map(soe_commonlib_shim::world::ActorValue)
    }

    fn default_object(&self, key: [u8; 4]) -> Option<u32> {
        crate::footsteps::Footsteps::new(&self.order)
            .default_object(key)
            .ok()
            .flatten()
            .map(|id| self.order.runtime_id(id))
    }
}

/// An impulse queued by a Papyrus reference native for a host actor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActorImpulse {
    pub actor: GlobalFormId,
    /// Skyrim-space vector in game units per second.
    pub vector: [f32; 3],
}

/// A latent gift menu requested by `Actor.ShowGiftMenu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftMenuRequest {
    pub latent: soe_papyrus::vm::LatentId,
    pub giver: GlobalFormId,
    pub source: GlobalFormId,
    pub target: GlobalFormId,
    pub filter: Option<GlobalFormId>,
    pub show_stolen: bool,
    pub use_favor_points: bool,
    pub favor_points: i32,
}

/// A transient presentation action requested by a Papyrus Game native.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameUiRequest {
    /// Open the character race and sex editor.
    ShowRaceMenu,
    /// Trigger a screen blood overlay with the authored intensity value.
    TriggerScreenBlood(i32),
    /// Ask the host to play a Bink file with the authored playback options.
    PlayBink {
        file_name: String,
        options: BinkOptions,
    },
}

/// Playback options supplied to `Game.PlayBink`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BinkOptions {
    /// Whether user input can interrupt playback.
    pub interruptible: bool,
    /// Whether video audio is muted.
    pub mute_audio: bool,
    /// Whether game music is muted.
    pub mute_music: bool,
    /// Whether the host displays letterboxing.
    pub letterbox: bool,
}

/// A transient camera shake requested by `Game.ShakeCamera`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraShake {
    pub source: GlobalFormId,
    pub strength: f32,
    pub duration: f32,
}

impl World {
    /// Set the current save character ID used by deterministic SPID rules.
    ///
    /// The host supplies the ID from the active save identity. New games use
    /// zero until the host sets one.
    pub fn set_player_character_id(&mut self, id: u64) {
        self.player_character_id = id;
    }

    /// Return the current save character ID used by deterministic SPID rules.
    #[must_use]
    pub const fn player_character_id(&self) -> u64 {
        self.player_character_id
    }

    pub fn new(order: LoadOrder) -> World {
        Self::from_shared_order(std::sync::Arc::new(order))
    }

    /// Construct a world with reproducible, seed-derived random streams.
    pub fn new_with_seed(order: LoadOrder, seed: u64) -> World {
        let mut world = Self::new(order);
        world.reset_random_seed(seed);
        world
    }

    /// Reset the existing gameplay random streams for a recorded input session.
    pub fn reset_random_seed(&mut self, seed: u64) {
        self.rng = if seed == 0 { 1 } else { seed };
        self.rng_streams = rng::derive_streams(seed);
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
            actor_kill_events: EventSource::default(),
            critical_hit_events: EventSource::default(),
            order,
            record_overrides: BTreeMap::new(),
            spatial_change_payloads: BTreeMap::new(),
            created_base_records: BTreeMap::new(),
            unresolved_records: BTreeMap::new(),
            editor_ids,
            objects: HashMap::new(),
            handles: HashMap::new(),
            refs: HashMap::new(),
            light_references: std::cell::RefCell::default(),
            trigger_occupants: BTreeMap::new(),
            trigger_index: None,
            actors: HashMap::new(),
            follower_services: BTreeMap::new(),
            quests: HashMap::new(),
            quest_start_stop_events: soe_commonlib_shim::events::EventSource::default(),
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
            papyrus_instructions_last_slice: 0,
            exterior_cell_loads_last_update: 0,
            loaded_3d_references: std::collections::HashSet::new(),
            attached_cells: std::collections::HashSet::new(),
            pending_cell_resets: std::collections::BTreeSet::new(),
            pending_unload: None,
            next_dynamic: 0x1000,
            player: None,
            crosshair_reference: None,
            grabbed_reference: None,
            player_first_person: true,
            player_character_id: 0,
            player_level: 1,
            training_sessions_this_level: 0,
            training_session_level: 1,
            pending_training_menu: None,
            rng: 0x9E37_79B9_7F4A_7C15,
            rng_streams: rng::derive_streams(0x9E37_79B9_7F4A_7C15),
            clock: GameClock::default(),
            menu_mode: false,
            line_of_sight: BTreeSet::new(),
            frame_rate: performance::FrameRateState::default(),
            pending: Vec::new(),
            host_requests: Vec::new(),
            spell_art_requests: Vec::new(),
            preloaded_spell_art: BTreeSet::new(),
            story_manager_events: Vec::new(),
            pending_story_events: Vec::new(),
            timers: Vec::new(),
            notifications: Vec::new(),
            native_errors: Vec::new(),
            menu_requests: Vec::new(),
            message_boxes: Vec::new(),
            screenshot_requests: Vec::new(),
            help_message_events: BTreeSet::new(),
            animation_events: Vec::new(),
            actor_impulses: Vec::new(),
            animation_requests: Vec::new(),
            animation_waits: Vec::new(),
            animation_host_notices: Vec::new(),
            animation_completions: Vec::new(),
            user_logs: BTreeMap::new(),
            pending_user_log_writes: Vec::new(),
            debug_host_requests: Vec::new(),
            player_control_flags: [true; 8],
            player_control_pov_type: 0,
            player_controls: true,
            camera_controls: CameraControls::default(),
            debug_god_mode: false,
            debug_ai_enabled: true,
            debug_collisions_enabled: true,
            debug_foot_ik: true,
            debug_ref_positions: BTreeSet::new(),
            ini: HashMap::new(),
            ai: crate::ai::AiWorld::default(),
            mod_data: crate::moddata::ModDataState::default(),
            difficulty: crate::combat::Difficulty::default(),
            settings: HashMap::new(),
            inventory_filters: HashMap::new(),
            gift_menu: None,
            sleep_listeners: Vec::new(),
            stats_listeners: Vec::new(),
            animation_listeners: Vec::new(),
            animation_waiters: Vec::new(),
            completed_animation_waits: Vec::new(),
            los_listeners: Vec::new(),
            stats: BTreeMap::new(),
            fast_travel: true,
            player_ai_driven: false,
            in_chargen: false,
            chargen_disable_saving: false,
            chargen_disable_waiting: false,
            chargen_show_controls_disabled_message: false,
            hud_cart_mode: false,
            game_ui_requests: Vec::new(),
            beast_form: false,
            camera_look: [0.0, 1.0, 0.0],
            camera_shakes: Vec::new(),
            words_taught: BTreeSet::new(),
            words_unlocked: BTreeSet::new(),
            learned_enchantment_effects: BTreeSet::new(),
            achievements: Vec::new(),
            last_ridden_horse: None,
            perk_points: 0,
            scenes: BTreeSet::new(),
            scene_runs: BTreeMap::new(),
            sound_instances: BTreeMap::new(),
            next_sound: 0,
            effect_shader_requests: Vec::new(),
            reference_physics_requests: Vec::new(),
            music_types: Vec::new(),
            spell_casts: Vec::new(),
            dialogue_index: std::sync::OnceLock::new(),
            quest_alias_candidates: std::sync::OnceLock::new(),
            dialogue: crate::dialogue::State::default(),
            known_dialogue_topics: BTreeSet::new(),
            duplicate_npc_perk_warnings: BTreeSet::new(),
            commonlib_events: soe_commonlib_shim::events::ScriptEventSourceHolder::default(),
            localized_strings: BTreeMap::new(),
            active_effects: BTreeMap::new(),
            visual_effect_attachments: BTreeMap::new(),
            visual_effects: BTreeMap::new(),
            cloak_effects: BTreeMap::new(),
            next_effect: 1,
            spell_casts_in_progress: BTreeMap::new(),
            vampire_lord_forms: BTreeMap::new(),
            brewed_potions: BTreeMap::new(),
            concentration_cast: None,
            telekinesis_grab: None,
            next_visual_effect: 1,
            image_space: image_space::ImageSpaceState::default(),
            sun_gaze_modifier: None,
            sun_gazing_last_tick: false,
            weather_image_space: None,
            shader_particles: shader_particles::ShaderParticleState::default(),
            effect_shaders: effect_shader::EffectShaderState::default(),
            detection: crate::detection::DetectionState::default(),
            story_history: BTreeMap::new(),
            story_selection: BTreeMap::new(),
            detection_events: Vec::new(),
            weather: weather::WeatherState::default(),
        }
    }

    pub fn order(&self) -> &LoadOrder {
        &self.order
    }

    /// The CommonLib-compatible channel raised after a non-silent actor kill.
    pub fn actor_kill_events(&mut self) -> &mut EventSource<ActorKill> {
        &mut self.actor_kill_events
    }

    /// The CommonLib-compatible channel for validated critical-hit reports.
    pub fn critical_hit_events(&mut self) -> &mut EventSource<CriticalHit> {
        &mut self.critical_hit_events
    }

    pub(crate) fn event_form_id(id: GlobalFormId) -> EventFormId {
        EventFormId((u64::from(id.file) << 32) | u64::from(id.object))
    }

    /// Queue a host screenshot request for the supplied path.
    pub fn request_screenshot(&mut self, path: String) {
        self.screenshot_requests.push(path);
    }

    /// Take pending host screenshot paths in request order.
    pub fn take_screenshot_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.screenshot_requests)
    }

    /// Drain operations that must be carried out by the active host.
    ///
    /// # Errors
    /// This method does not fail.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn take_host_requests(&mut self) -> Vec<HostRequest> {
        std::mem::take(&mut self.host_requests)
    }

    /// Take pending spell art operations in Papyrus call order.
    ///
    /// # Panics
    ///
    /// This method does not panic.
    pub fn take_spell_art_requests(&mut self) -> Vec<SpellArtRequest> {
        std::mem::take(&mut self.spell_art_requests)
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

#[cfg(test)]
mod shm_05_tests {
    use super::*;
    use soe_commonlib_shim::world::{ActorValue, ActorValueWorldState, BgsDefaultObjectManager};

    #[test]
    fn shm_05_game_world_exposes_live_actor_values_and_record_defaults() {
        let mut world = World::new(LoadOrder::new());
        let actor = GlobalFormId {
            file: DYNAMIC_FILE,
            object: 0x1234,
        };
        let mut state = ActorState::default();
        state
            .values
            .set_base("health", 100.0)
            .expect("valid actor value fixture");
        world.actors.insert(actor, state);
        let mut reference = RefState::new(None);
        reference.is_actor = true;
        world.refs.insert(actor, reference);

        let runtime_actor = world.order.runtime_id(actor);
        assert_eq!(
            ActorValueWorldState::actor_value_name(&world, ActorValue::HEALTH).as_deref(),
            Some("health")
        );
        assert_eq!(
            ActorValueWorldState::actor_value_by_name(&world, "HEALTH"),
            Some(ActorValue::HEALTH)
        );
        assert_eq!(
            ActorValueWorldState::actor_value(&world, runtime_actor, ActorValue::HEALTH),
            Some(100.0)
        );
        assert!(ActorValueWorldState::set_actor_value(
            &mut world,
            runtime_actor,
            ActorValue::HEALTH,
            75.0
        ));
        assert_eq!(
            ActorValueWorldState::actor_value(&world, runtime_actor, ActorValue::HEALTH),
            Some(75.0)
        );
        assert_eq!(BgsDefaultObjectManager::new(&world).get(*b"GOLD"), None);
    }
}
