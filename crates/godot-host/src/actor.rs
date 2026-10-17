// SPDX-License-Identifier: GPL-3.0-or-later

//! Loading one animated character out of the game's archives.
//!
//! A character is four files that reference each other only by name: a
//! skeleton, a behavior graph, the clips that graph's generators name, and a
//! skinned mesh. [`ActorAssets::load`] gathers them through the same
//! [`soe_bsa::Vfs`] the renderer reads meshes from, so a loose file replacing
//! an archived one wins here exactly as it does everywhere else.
//!
//! Nothing in this module touches Godot, so the whole load runs in a plain
//! test. [`crate::godot_actor`] turns the result into `RenderingServer` calls.
//! Race project links use the `soe-hkx` readers, whose layouts follow the local
//! serde-hkx `hkbProjectStringData` and `hkbCharacterStringData` class tables.

use soe_bsa::Vfs;
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::character::{CharacterData, ProjectData};
use soe_hkx::interp::{Clip, ClipLibrary};
use soe_hkx::ragdoll::Ragdoll;
use soe_hkx::skeleton::{AnimationContainer, Bone as HkxBone, Skeleton};
use soe_hkx::Packfile;
use soe_nif::mesh::Skin;
use soe_nif::types::Transform as NifTransform;

use crate::data_source::{mesh_data_from, SurfaceTextures};
use crate::mesh::{build_surface, MeshOptions, SurfaceArrays};
use crate::skinned::BoneMap;

/// Where the vanilla human character's files live inside the archives.
pub const CHARACTER_ROOT: &str = "meshes/actors/character/";

/// The vanilla human skeleton, relative to [`CHARACTER_ROOT`].
pub const HUMAN_SKELETON: &str = "character assets/skeleton.hkx";

/// The locomotion graph, relative to [`CHARACTER_ROOT`]. It is the graph the
/// `behavior-2` work proved out, and it names the idle and walk clips.
pub const LOCOMOTION_BEHAVIOR: &str = "behaviors/mt_behavior.hkx";

/// The human behavior graph that connects locomotion, stagger and combat.
pub const MASTER_BEHAVIOR: &str = "behaviors/0_master.hkx";

/// Vanilla one-handed melee behavior, with authored attack markers.
pub const ONE_HAND_BEHAVIOR: &str = "behaviors/1hm_behavior.hkx";

/// Render transform supplied by the shared world's actor placement.
pub fn npc_transform(placement: soe_game::world::RefPlacement) -> crate::units::GodotTransform {
    crate::units::placement_to_godot(
        body_heading_matrix(placement.rotation),
        placement.position,
        placement.scale,
    )
}

/// Actor bodies stay upright while the view and head can pitch independently.
pub(crate) fn body_heading_matrix(rotation: [f32; 3]) -> [[f32; 3]; 3] {
    crate::units::skyrim_euler_matrix([0.0, 0.0, rotation[2]])
}

/// Authored locomotion inputs derived from consecutive rendered placements.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcMotion {
    /// Horizontal Skyrim units per second, used for walk and run graph inputs.
    pub speed: f32,
    /// Total displacement per second, including gravity and vertical corrections.
    pub world_speed: f32,
    /// Degrees relative to the actor's heading.
    pub direction: f32,
    /// Heading change in degrees per second.
    pub turn: f32,
}

impl NpcMotion {
    /// Sample movement using the render clock. Invalid poses or rates produce no sample.
    pub fn between(
        previous: soe_game::world::RefPlacement,
        current: soe_game::world::RefPlacement,
        delta: f32,
    ) -> Option<Self> {
        if !delta.is_finite()
            || delta <= 0.0
            || !previous
                .position
                .iter()
                .chain(&previous.rotation)
                .chain(&current.position)
                .chain(&current.rotation)
                .all(|value| value.is_finite())
        {
            return None;
        }
        let displacement: [f64; 3] = std::array::from_fn(|axis| {
            f64::from(current.position[axis]) - f64::from(previous.position[axis])
        });
        let horizontal = displacement[0].hypot(displacement[1]);
        let speed = (horizontal / f64::from(delta)) as f32;
        let world_speed = (horizontal.hypot(displacement[2]) / f64::from(delta)) as f32;
        let wrap = |angle: f64| {
            (angle + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
        };
        let direction = if speed > 0.1 {
            wrap(displacement[0].atan2(displacement[1]) - f64::from(current.rotation[2]))
                .to_degrees() as f32
        } else {
            0.0
        };
        let turn = (wrap(f64::from(current.rotation[2]) - f64::from(previous.rotation[2]))
            .to_degrees()
            / f64::from(delta)) as f32;
        [speed, world_speed, direction, turn]
            .into_iter()
            .all(f32::is_finite)
            .then_some(Self {
                speed,
                world_speed,
                direction,
                turn,
            })
    }
}

/// What went wrong assembling a character.
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("actor appearance: {0}")]
    Appearance(#[from] soe_esm::Error),
    #[error("loaded rig does not contain skin bones: {bones:?}")]
    RigMismatch { bones: Vec<String> },
    #[error("reading {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: soe_bsa::Error,
    },
    #[error("parsing {path}: {source}")]
    Havok {
        path: String,
        #[source]
        source: soe_hkx::Error,
    },
    #[error("parsing {path}: {source}")]
    Nif {
        path: String,
        #[source]
        source: soe_nif::Error,
    },
    #[error("invalid body partitions in {path}: {reason}")]
    Partitions { path: String, reason: String },
    #[error("{path} holds no skeleton")]
    NoSkeleton { path: String },
    #[error("reading race animation project {path}: {reason}")]
    Project { path: String, reason: String },
    #[error("race {race} has no {asset} animation model for this actor")]
    MissingRaceModel {
        race: soe_esm::GlobalFormId,
        asset: &'static str,
    },
    #[error("{path} holds no skinned shape, so there is nothing to animate")]
    NotSkinned { path: String },
    #[error("building a surface for {path}: {source}")]
    Surface {
        path: String,
        #[source]
        source: crate::mesh::MeshError,
    },
}

/// One skinned shape of a character, ready to upload.
pub struct ActorSurface {
    /// The shape's name in the `NIF`, for reporting.
    pub name: String,
    /// HDPT type when this is a selected head surface.
    pub head_part_type: Option<u32>,
    /// Exported face tint sampled alongside the head's diffuse map.
    pub face_tint: Option<(String, std::sync::Arc<crate::data_source::TextureData>)>,
    /// Authored FaceGen detail map from texture slot 3.
    pub face_detail: Option<(String, std::sync::Arc<crate::data_source::TextureData>)>,
    /// Authored SkinTint or HairTint RGB multiplier.
    pub tint_color: Option<[f32; 3]>,
    pub arrays: SurfaceArrays,
    /// Expression blend-shape deltas in the order uploaded to Godot.
    pub expression_shapes: Vec<ExpressionShape>,
    /// Textures resolved from the material paths in the mesh.
    pub textures: SurfaceTextures,
    /// The shape's own bone list, carrying the inverse bind transforms.
    pub skin: Skin,
    /// That list resolved against the skeleton.
    pub map: BoneMap,
    /// The shape node's own accumulated transform in the `NIF`. A skinned
    /// shape normally sits at identity because its vertices are already in
    /// skin space, so anything else here is worth knowing about.
    pub node_offset: [f32; 3],
    /// Body part partitions the shape is split into.
    pub partitions: usize,
    /// Whether the partition-local bone index correction applied, see
    /// [`partition_bone_indices`].
    pub remapped: bool,
}

impl ActorSurface {
    #[cfg(any(feature = "godot", test))]
    pub(crate) fn release_uploaded_images(&mut self) {
        self.textures = SurfaceTextures::default();
        self.face_tint = None;
        self.face_detail = None;
    }
}

/// One named CK expression target in Godot surface coordinates.
#[derive(Clone)]
pub struct ExpressionShape {
    /// The CK target name used by `SetExpressionOverride`.
    pub name: String,
    /// Relative vertex offsets for Godot's blend-shape API.
    pub positions: Vec<[f32; 3]>,
    /// Relative normal offsets when the surface carries normals.
    pub normals: Vec<[f32; 3]>,
}

/// A character's skeleton, graph, clips and skinned shapes.
pub struct ActorAssets {
    pub skeleton: Skeleton,
    pub ragdoll: Option<Ragdoll>,
    pub graph: BehaviorGraph,
    pub clips: ClipLibrary,
    pub surfaces: Vec<ActorSurface>,
    /// Archive-relative race skeleton path selected for this actor.
    pub skeleton_path: String,
    /// Model path named by the race record, usually a NIF skeleton.
    pub skeleton_model_path: String,
    /// Archive-relative race behavior graph path selected for this actor.
    pub behavior_path: String,
    /// Archive-relative project file selected by the race behavior model.
    pub project_path: Option<String>,
    /// Race and sex used when the live actor selected this rig.
    pub actor_race: Option<soe_esm::GlobalFormId>,
    pub actor_female: Option<bool>,
    /// Named property defaults from the race's selected character file.
    pub character_properties: Vec<(String, i32)>,
    /// Clip names the graph asked for that the archives do not hold.
    pub missing_clips: Vec<String>,
    /// Referenced behavior files that linking could not find.
    pub unresolved_references: Vec<String>,
}

/// Prepared geometry and its source view. The consumer commits both only
/// after upload succeeds, preserving its previous view on any load error.
pub struct AppearanceUpdate {
    pub appearance: soe_game::appearance::ActorAppearance,
    pub surfaces: Vec<ActorSurface>,
}

/// Transient render cache. Failed views are remembered so unchanged bad assets
/// do not cause repeated I/O or repeated errors. Rebinding starts a fresh cache.
#[derive(Default)]
pub struct AppearanceCache {
    last_attempt: Option<soe_game::appearance::ActorAppearance>,
}

impl AppearanceCache {
    pub fn prepare(
        &mut self,
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
        vfs: &Vfs,
        skeleton: &Skeleton,
    ) -> Result<Option<AppearanceUpdate>, ActorError> {
        let appearance = world.actor_appearance(actor)?;
        self.prepare_appearance(appearance, vfs, skeleton)
    }

    /// Assemble only first-person addon models, retaining the same live selection rules.
    pub fn prepare_first_person(
        &mut self,
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
        vfs: &Vfs,
        skeleton: &Skeleton,
    ) -> Result<Option<AppearanceUpdate>, ActorError> {
        let mut appearance = world.actor_appearance(actor)?;
        for part in &mut appearance.parts {
            part.model.clone_from(&part.first_person_model);
        }
        appearance.head_parts.clear();
        appearance.facegen = None;
        self.prepare_appearance(appearance, vfs, skeleton)
    }

    fn prepare_appearance(
        &mut self,
        appearance: soe_game::appearance::ActorAppearance,
        vfs: &Vfs,
        skeleton: &Skeleton,
    ) -> Result<Option<AppearanceUpdate>, ActorError> {
        if self.last_attempt.as_ref() == Some(&appearance) {
            return Ok(None);
        }
        self.last_attempt = Some(appearance.clone());
        let surfaces = ActorAssets::appearance_surfaces(vfs, &appearance, skeleton)?;
        let bones: std::collections::BTreeSet<_> = surfaces
            .iter()
            .flat_map(|surface| surface.map.unmatched.iter().cloned())
            .collect();
        if !bones.is_empty() {
            return Err(ActorError::RigMismatch {
                bones: bones.into_iter().collect(),
            });
        }
        Ok(Some(AppearanceUpdate {
            appearance,
            surfaces,
        }))
    }
}

fn read(vfs: &Vfs, path: &str) -> Result<Vec<u8>, ActorError> {
    vfs.read(path).map_err(|source| ActorError::Read {
        path: path.to_string(),
        source,
    })
}

fn packfile(bytes: &[u8], path: &str) -> Result<Packfile, ActorError> {
    Packfile::parse(bytes).map_err(|source| ActorError::Havok {
        path: path.to_string(),
        source,
    })
}

impl ActorAssets {
    /// Create a graph with the property defaults authored by its character file.
    pub fn interpreter(&self) -> soe_hkx::interp::Interpreter<'_> {
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::ActorGraph);
        let mut vm = soe_hkx::interp::Interpreter::new(
            &self.graph,
            &self.clips,
            self.skeleton.reference_pose.clone(),
        );
        for (name, word) in &self.character_properties {
            if let Some(index) = vm.character_properties.index(name) {
                vm.character_properties.set_word(index, *word);
            }
        }
        vm
    }

    /// Create a streamed NPC graph with its sex and host locomotion inputs.
    pub fn npc_interpreter(&self, female: bool) -> soe_hkx::interp::Interpreter<'_> {
        let mut vm = self.interpreter();
        vm.character_properties
            .set_by_name("IsFemale", f32::from(female));
        for name in ["IsMale", "IsMale00", "IsMale01"] {
            vm.character_properties
                .set_by_name(name, f32::from(!female));
        }
        vm.character_properties.set_by_name("IsThirdPerson", 1.0);
        for (name, value) in [
            ("IsNPC", 1.0),
            ("IsPlayer", 0.0),
            ("iIsPlayer", 0.0),
            ("IsFirstPerson", 0.0),
            ("Speed", 0.0),
            ("Direction", 0.0),
            ("TurnDelta", 0.0),
        ] {
            vm.variables.set_by_name(name, value);
        }
        vm
    }

    /// Load the vanilla human character: its skeleton, the locomotion graph
    /// with its references linked, every clip that graph names which the
    /// archives hold, and the skinned shapes of `mesh_path`.
    ///
    /// Missing clips and unresolved references are collected rather than
    /// refused, because vanilla names files it does not ship and the
    /// interpreter already reports them.
    ///
    /// # Errors
    ///
    /// [`ActorError`] when a required file is missing or will not parse, or
    /// when the mesh carries no skinned shape.
    pub fn load(vfs: &Vfs, mesh_path: &str) -> Result<ActorAssets, ActorError> {
        Self::load_with_behavior(vfs, mesh_path, LOCOMOTION_BEHAVIOR)
    }

    /// Load the human character with an explicitly selected behavior graph.
    ///
    /// # Errors
    /// A required rig, graph, animation or mesh cannot be read or decoded.
    pub fn load_with_behavior(
        vfs: &Vfs,
        mesh_path: &str,
        behavior: &str,
    ) -> Result<ActorAssets, ActorError> {
        let (skeleton, ragdoll) = Self::load_skeleton(vfs)?;
        let behavior = rooted_asset_path(CHARACTER_ROOT, behavior);
        let (mut graph, unresolved_references) = Self::load_graph(vfs, &behavior)?;
        let (clips, missing_clips) = Self::load_clips(vfs, &graph, CHARACTER_ROOT);
        crate::combat::contact_annotation_triggers(&mut graph, &clips);
        let surfaces = Self::load_surfaces(vfs, mesh_path, &skeleton)?;
        let skeleton_path = HUMAN_SKELETON.to_string();
        let skeleton_model_path = HUMAN_SKELETON.to_string();
        let behavior_path = behavior;
        Ok(ActorAssets {
            skeleton,
            ragdoll,
            graph,
            clips,
            surfaces,
            skeleton_path,
            skeleton_model_path,
            behavior_path,
            project_path: None,
            actor_race: None,
            actor_female: None,
            character_properties: Vec::new(),
            missing_clips,
            unresolved_references,
        })
    }

    /// Load the rig, behavior and visible armor selected by an actor's records.
    pub fn load_for_actor(
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
        vfs: &Vfs,
    ) -> Result<ActorAssets, ActorError> {
        let base = world
            .reference_ref(actor)
            .and_then(|reference| reference.base);
        let _object = soe_trace::LoadScope::object(format!(
            "base:{} actor:{actor}",
            base.map_or_else(|| "unknown".to_owned(), |base| base.to_string())
        ));
        let rig = world
            .actor_animation_rig(actor)?
            .ok_or(ActorError::MissingRaceModel {
                race: actor,
                asset: "race",
            })?;
        let race = world
            .record(rig.race)
            .ok_or(ActorError::MissingRaceModel {
                race: rig.race,
                asset: "record",
            })?
            .clone();
        let mut assets =
            Self::load_for_race(vfs, &race, rig.female, &world.actor_appearance(actor)?)?;
        assets.actor_race = Some(rig.race);
        Ok(assets)
    }

    /// Load the player's first-person character under the selected race project root.
    /// The first-person character file names its own rig and behavior graph.
    pub fn load_player_first_person(
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
        vfs: &Vfs,
    ) -> Result<ActorAssets, ActorError> {
        let rig = world
            .actor_animation_rig(actor)?
            .ok_or(ActorError::MissingRaceModel {
                race: actor,
                asset: "race",
            })?;
        let race = world.record(rig.race).ok_or(ActorError::MissingRaceModel {
            race: rig.race,
            asset: "record",
        })?;
        let definition = load_race_character(vfs, race, rig.female)?;
        let root = format!("{}_1stperson/", actor_root(&definition.behavior_path));
        let character_path = format!("{root}characters/firstperson.hkx");
        let bytes = read(vfs, &character_path)?;
        let character =
            CharacterData::from_packfile(&packfile(&bytes, &character_path)?).map_err(|error| {
                ActorError::Project {
                    path: character_path,
                    reason: error.to_string(),
                }
            })?;
        let character_properties = character
            .character_property_names
            .iter()
            .cloned()
            .zip(
                character
                    .character_property_values
                    .as_ref()
                    .map(|values| values.word_values.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .copied(),
            )
            .collect();
        let skeleton_path = rooted_asset_path(&root, &character.rig_name);
        let behavior_path = rooted_asset_path(&root, &character.behavior_filename);
        let (skeleton, _) = Self::load_skeleton_at(vfs, &skeleton_path)?;
        let (mut graph, unresolved_references) = Self::load_graph(vfs, &behavior_path)?;
        let (clips, missing_clips) = Self::load_clips(vfs, &graph, &root);
        crate::combat::contact_annotation_triggers(&mut graph, &clips);
        let mut appearance = world.actor_appearance(actor)?;
        for part in &mut appearance.parts {
            part.model.clone_from(&part.first_person_model);
        }
        appearance.head_parts.clear();
        appearance.facegen = None;
        let surfaces = Self::appearance_surfaces(vfs, &appearance, &skeleton)?;
        let mut assets = ActorAssets {
            skeleton,
            ragdoll: None,
            graph,
            clips,
            surfaces,
            skeleton_path,
            skeleton_model_path: format!("{root}skeleton.nif"),
            behavior_path,
            project_path: Some(definition.project_path),
            actor_race: Some(rig.race),
            actor_female: Some(rig.female),
            character_properties,
            missing_clips,
            unresolved_references,
        };
        Self::extend_race_bones(vfs, &mut assets)?;
        Ok(assets)
    }

    /// Load a sex-specific rig and appearance from a winning RACE record.
    pub fn load_for_race(
        vfs: &Vfs,
        race: &soe_esm::Record,
        female: bool,
        appearance: &soe_game::appearance::ActorAppearance,
    ) -> Result<ActorAssets, ActorError> {
        let race_id = soe_esm::GlobalFormId {
            file: 0,
            object: race.form_id,
        };
        let RaceCharacter {
            skeleton_model_path,
            project_path,
            character,
            skeleton_path,
            behavior_path,
        } = load_race_character(vfs, race, female)?;
        let root = actor_root(&project_path);
        let (skeleton, ragdoll) = Self::load_skeleton_at(vfs, &skeleton_path)?;
        let (mut graph, unresolved_references) = Self::load_graph(vfs, &behavior_path)?;
        let (clips, missing_clips) = Self::load_clips(vfs, &graph, &root);
        crate::combat::contact_annotation_triggers(&mut graph, &clips);
        let mut assets = ActorAssets {
            skeleton,
            ragdoll,
            graph,
            clips,
            surfaces: Vec::new(),
            skeleton_path,
            skeleton_model_path,
            behavior_path,
            project_path: Some(project_path),
            actor_race: Some(race_id),
            actor_female: Some(female),
            character_properties: character
                .character_property_names
                .iter()
                .cloned()
                .zip(
                    character
                        .character_property_values
                        .as_ref()
                        .map(|values| values.word_values.as_slice())
                        .unwrap_or_default()
                        .iter()
                        .copied(),
                )
                .collect(),
            missing_clips,
            unresolved_references,
        };
        if appearance
            .parts
            .iter()
            .any(|part| part.visible_slots & (1 << 9) != 0)
        {
            assets.complete_race_skeleton(vfs)?;
        }
        assets.surfaces = Self::appearance_surfaces(vfs, appearance, &assets.skeleton)?;
        Self::extend_race_bones(vfs, &mut assets)?;
        let bones: std::collections::BTreeSet<_> = assets
            .surfaces
            .iter()
            .flat_map(|surface| surface.map.unmatched.iter().cloned())
            .collect();
        if !bones.is_empty() {
            return Err(ActorError::RigMismatch {
                bones: bones.into_iter().collect(),
            });
        }
        Ok(assets)
    }

    /// Retain every authored rig node before sharing a rig across appearances.
    /// Later outfits can bind bones the first appearance did not use.
    pub(crate) fn complete_race_skeleton(&mut self, vfs: &Vfs) -> Result<(), ActorError> {
        let bytes = read(vfs, &self.skeleton_model_path)?;
        let file = soe_nif::parse(&bytes).map_err(|source| ActorError::Nif {
            path: self.skeleton_model_path.clone(),
            source,
        })?;
        let mut nodes = std::collections::BTreeMap::new();
        let mut visited = std::collections::BTreeSet::new();
        for root in &file.roots {
            collect_rig_nodes(&file, *root, None, &mut visited, &mut nodes);
        }
        extend_skeleton(&mut self.skeleton, &file, &nodes.into_keys().collect())
            .map_err(|bones| ActorError::RigMismatch { bones })
    }

    fn extend_race_bones(vfs: &Vfs, assets: &mut ActorAssets) -> Result<(), ActorError> {
        let mut missing: std::collections::BTreeSet<_> = assets
            .surfaces
            .iter()
            .flat_map(|surface| surface.map.unmatched.iter().cloned())
            .collect();
        if missing.is_empty() && !vfs.exists(&assets.skeleton_model_path) {
            return Ok(());
        }
        let bytes = read(vfs, &assets.skeleton_model_path)?;
        let file = soe_nif::parse(&bytes).map_err(|source| ActorError::Nif {
            path: assets.skeleton_model_path.clone(),
            source,
        })?;
        let mut nodes = std::collections::BTreeMap::new();
        let mut visited = std::collections::BTreeSet::new();
        for root in &file.roots {
            collect_rig_nodes(&file, *root, None, &mut visited, &mut nodes);
        }
        for name in [
            "WeaponSword",
            "WeaponDagger",
            "WeaponAxe",
            "WeaponMace",
            "WeaponBack",
            "WeaponBow",
            "ShieldBack",
        ] {
            if nodes.contains_key(name) && assets.skeleton.bone_index(name).is_none() {
                missing.insert(name.to_string());
            }
        }
        if missing.is_empty() {
            return Ok(());
        }
        extend_skeleton(&mut assets.skeleton, &file, &missing)
            .map_err(|bones| ActorError::RigMismatch { bones })?;
        for surface in &mut assets.surfaces {
            surface.map = BoneMap::build(&surface.skin.bones, &assets.skeleton);
        }
        Ok(())
    }

    fn load_skeleton(vfs: &Vfs) -> Result<(Skeleton, Option<Ragdoll>), ActorError> {
        Self::load_skeleton_at(vfs, &format!("{CHARACTER_ROOT}{HUMAN_SKELETON}"))
    }

    fn load_skeleton_at(vfs: &Vfs, path: &str) -> Result<(Skeleton, Option<Ragdoll>), ActorError> {
        let bytes = read(vfs, path)?;
        let _parse = soe_trace::LoadScope::new("skeleton_parse", soe_trace::LoadCause::Parse, path);
        let pf = packfile(&bytes, path)?;
        let container =
            AnimationContainer::from_packfile(&pf).map_err(|source| ActorError::Havok {
                path: path.to_string(),
                source,
            })?;
        let ragdoll = match Ragdoll::from_packfile(&pf) {
            Ok(ragdoll) => Some(ragdoll),
            Err(soe_hkx::Error::NullPointer("hkaRagdollInstance")) => None,
            Err(source) => {
                return Err(ActorError::Havok {
                    path: path.to_string(),
                    source,
                })
            }
        };
        let skeleton =
            container
                .skeletons
                .into_iter()
                .next()
                .ok_or_else(|| ActorError::NoSkeleton {
                    path: path.to_string(),
                })?;
        Ok((skeleton, ragdoll))
    }

    fn load_graph(vfs: &Vfs, path: &str) -> Result<(BehaviorGraph, Vec<String>), ActorError> {
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::ActorGraph);
        let bytes = read(vfs, path)?;
        let _parse =
            soe_trace::LoadScope::new("behaviour_graph", soe_trace::LoadCause::Parse, path);
        let pf = packfile(&bytes, path)?;
        let mut graph = BehaviorGraph::from_packfile(&pf).map_err(|source| ActorError::Havok {
            path: path.to_string(),
            source,
        })?;
        // A referenced behavior may itself reference another, so the walk
        // repeats. Eight rounds is what the gated hkx test uses and the
        // vanilla graph settles in two. See docs/hkx.md.
        graph.link_all(8, |name| {
            let sub_path = behavior_reference_path(path, name);
            let bytes = vfs.read(&sub_path).ok()?;
            let pf = Packfile::parse(&bytes).ok()?;
            BehaviorGraph::from_packfile(&pf).ok()
        });
        let unresolved = graph
            .unresolved_references()
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        Ok((graph, unresolved))
    }

    fn load_clips(vfs: &Vfs, graph: &BehaviorGraph, root: &str) -> (ClipLibrary, Vec<String>) {
        let _owner = crate::allocations::OwnerScope::new(crate::allocations::Owner::ActorClips);
        let mut clips = ClipLibrary::new();
        let mut missing = Vec::new();
        for name in graph.clip_animation_names() {
            if clips.get(&name).is_some() {
                continue;
            }
            let path = rooted_asset_path(root, &name);
            let Ok(bytes) = vfs.read(&path) else {
                missing.push(name);
                continue;
            };
            let Ok(pf) = Packfile::parse(&bytes) else {
                missing.push(name);
                continue;
            };
            let Ok(container) = AnimationContainer::from_packfile(&pf) else {
                missing.push(name);
                continue;
            };
            match Clip::from_container(&name, &container) {
                Ok(clip) => clips.insert(clip),
                Err(_) => missing.push(name),
            }
        }
        (clips, missing)
    }

    fn load_surfaces(
        vfs: &Vfs,
        mesh_path: &str,
        skeleton: &Skeleton,
    ) -> Result<Vec<ActorSurface>, ActorError> {
        Self::load_visible_surfaces(vfs, mesh_path, skeleton, 0)
    }

    /// Assemble the current third-person addon models on an existing rig.
    /// Record paths resolve through the same archive and loose-file overlay.
    /// BoneMap reports unmatched rig bones. Live consumers use
    /// [`AppearanceCache::prepare`] to reject those before replacing their geometry.
    pub fn appearance_surfaces(
        vfs: &Vfs,
        appearance: &soe_game::appearance::ActorAppearance,
        skeleton: &Skeleton,
    ) -> Result<Vec<ActorSurface>, ActorError> {
        Self::appearance_surfaces_with_mouth(vfs, appearance, skeleton, &[])
    }

    pub(crate) fn appearance_surfaces_with_mouth(
        vfs: &Vfs,
        appearance: &soe_game::appearance::ActorAppearance,
        skeleton: &Skeleton,
        mouth_targets: &[String],
    ) -> Result<Vec<ActorSurface>, ActorError> {
        let mut surfaces = Vec::new();
        for part in &appearance.parts {
            let Some(model) = &part.model else { continue };
            let path = model.replace('\\', "/");
            let path = if path.to_ascii_lowercase().starts_with("meshes/") {
                path
            } else {
                format!("meshes/{path}")
            };
            let attachment = (part.visible_slots & (1 << 9) != 0)
                .then(|| crate::combat::weapon_bones(soe_game::combat::Hand::Left))
                .and_then(|names| {
                    names
                        .iter()
                        .copied()
                        .find(|name| skeleton.bones.iter().any(|bone| bone.name == *name))
                });
            surfaces.extend(Self::load_attached_surfaces(
                vfs,
                &path,
                skeleton,
                part.authored_slots & !part.visible_slots,
                attachment,
                &[],
            )?);
        }
        let mut exported = if let Some(face) = &appearance.facegen {
            let path = format!(
                "meshes/actors/character/facegendata/facegeom/{}/{:08X}.nif",
                face.plugin, face.object
            );
            if vfs.exists(&path) {
                Self::load_attached_surfaces(
                    vfs,
                    &path,
                    skeleton,
                    0,
                    Some("NPC Head [Head]"),
                    mouth_targets,
                )?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        for part in &appearance.head_parts {
            let mut head = Vec::new();
            let mut remaining = Vec::new();
            for surface in exported.drain(..) {
                if surface.name.eq_ignore_ascii_case(&part.name) {
                    head.push(surface);
                } else {
                    remaining.push(surface);
                }
            }
            exported = remaining;
            let from_export = !head.is_empty();
            if from_export {
                if let Some(model) = &part.model {
                    let templates = Self::load_attached_surfaces(
                        vfs,
                        &asset_path(model),
                        skeleton,
                        0,
                        Some("NPC Head [Head]"),
                        mouth_targets,
                    )?;
                    for surface in &mut head {
                        if let Some(template) = templates.iter().find(|template| {
                            template.arrays.positions.len() == surface.arrays.positions.len()
                                && template.arrays.indices == surface.arrays.indices
                        }) {
                            surface.expression_shapes = template.expression_shapes.clone();
                        }
                    }
                }
            }
            if head.is_empty() {
                if let Some(model) = &part.model {
                    head = Self::load_attached_surfaces(
                        vfs,
                        &asset_path(model),
                        skeleton,
                        0,
                        Some("NPC Head [Head]"),
                        mouth_targets,
                    )?;
                }
            }
            for surface in &mut head {
                surface.head_part_type = Some(part.part_type);
                if from_export && part.part_type == 1 {
                    if let Some(face) = &appearance.facegen {
                        let path = format!(
                            "textures/actors/character/facegendata/facetint/{}/{:08X}.dds",
                            face.plugin, face.object
                        );
                        if vfs.exists(&path) {
                            let textures = crate::data_source::resolve_material_textures(
                                vfs,
                                &[crate::mesh::MaterialDesc {
                                    diffuse: Some(path.clone()),
                                    ..Default::default()
                                }],
                            );
                            if let Some(diffuse) = textures
                                .into_iter()
                                .next()
                                .and_then(|textures| textures.diffuse)
                            {
                                surface.face_tint = Some((path, diffuse));
                            }
                        }
                    }
                }
            }
            surfaces.extend(head);
        }
        Ok(surfaces)
    }

    fn load_visible_surfaces(
        vfs: &Vfs,
        mesh_path: &str,
        skeleton: &Skeleton,
        hidden_slots: u32,
    ) -> Result<Vec<ActorSurface>, ActorError> {
        Self::load_attached_surfaces(vfs, mesh_path, skeleton, hidden_slots, None, &[])
    }

    fn load_attached_surfaces(
        vfs: &Vfs,
        mesh_path: &str,
        skeleton: &Skeleton,
        hidden_slots: u32,
        attachment: Option<&str>,
        mouth_targets: &[String],
    ) -> Result<Vec<ActorSurface>, ActorError> {
        let bytes = read(vfs, mesh_path)?;
        let file = soe_nif::parse(&bytes).map_err(|source| ActorError::Nif {
            path: mesh_path.to_string(),
            source,
        })?;
        let _build = soe_trace::LoadScope::new(
            "actor_mesh_build",
            soe_trace::LoadCause::CpuBuild,
            mesh_path,
        );
        let mut meshes = file.meshes().map_err(|source| ActorError::Nif {
            path: mesh_path.to_string(),
            source,
        })?;
        for mesh in &mut meshes {
            for texture in &mut mesh.material.textures {
                let normalized = texture.replace('\\', "/");
                if normalized.to_ascii_lowercase().starts_with("actors/") {
                    *texture = format!("textures/{normalized}");
                }
            }
            if let Some(soe_nif::blocks::Block::BsTriShape(shape)) = file.blocks.get(mesh.block) {
                apply_dynamic_positions(shape, &mut mesh.positions).map_err(|reason| {
                    ActorError::Partitions {
                        path: mesh_path.to_owned(),
                        reason,
                    }
                })?;
            }
        }
        let mut out = Vec::new();
        for mesh in &meshes {
            if mesh.skin.is_none() {
                if let Some(bone) = attachment {
                    let mut surface = rigid_surface(mesh, bone, skeleton).map_err(|source| {
                        ActorError::Surface {
                            path: mesh_path.to_owned(),
                            source,
                        }
                    })?;
                    surface.expression_shapes =
                        expression_shapes(vfs, mesh_path, mesh, &surface.arrays, mouth_targets)?;
                    out.push(surface);
                }
                continue;
            }
            let Some(skin) = mesh.skin.clone() else {
                continue;
            };
            if mesh.bone_weights.is_empty() {
                continue;
            }
            let mut arrays = surface_of(mesh).map_err(|source| ActorError::Surface {
                path: mesh_path.to_string(),
                source,
            })?;
            let expression_shapes =
                expression_shapes(vfs, mesh_path, mesh, &arrays, mouth_targets)?;
            if let Some(indices) = visible_indices(&file, mesh, hidden_slots).map_err(|reason| {
                ActorError::Partitions {
                    path: mesh_path.to_owned(),
                    reason,
                }
            })? {
                arrays.indices = indices;
                if arrays.indices.is_empty() {
                    continue;
                }
            }
            let partitions = partition_count(&file, mesh);
            let remapped = match partition_bone_indices(&file, mesh) {
                Some(fixed) => {
                    arrays.bones = fixed;
                    true
                }
                None => false,
            };
            let map = BoneMap::build(&skin.bones, skeleton);
            out.push(ActorSurface {
                name: mesh.name.clone(),
                head_part_type: None,
                face_tint: None,
                face_detail: None,
                tint_color: authored_tint(mesh),
                arrays,
                expression_shapes,
                textures: SurfaceTextures::default(),
                skin,
                map,
                node_offset: [
                    mesh.transform.translation.x,
                    mesh.transform.translation.y,
                    mesh.transform.translation.z,
                ],
                partitions,
                remapped,
            });
            if mesh.material.kind
                == soe_nif::mesh::MaterialKind::Lighting(soe_nif::blocks::ShaderType::FaceTint)
            {
                if let Some(path) = mesh
                    .material
                    .textures
                    .get(3)
                    .filter(|path| !path.is_empty())
                {
                    let textures = crate::data_source::resolve_material_textures(
                        vfs,
                        &[crate::mesh::MaterialDesc {
                            diffuse: Some(path.clone()),
                            ..Default::default()
                        }],
                    );
                    if let (Some(surface), Some(data)) = (
                        out.last_mut(),
                        textures.into_iter().next().and_then(|t| t.diffuse),
                    ) {
                        surface.face_detail = Some((path.clone(), data));
                    }
                }
            }
        }
        if out.is_empty() && hidden_slots == 0 {
            return Err(ActorError::NotSkinned {
                path: mesh_path.to_string(),
            });
        }
        let materials: Vec<_> = out
            .iter()
            .map(|surface| surface.arrays.material.clone())
            .collect();
        for (surface, textures) in
            out.iter_mut()
                .zip(crate::data_source::resolve_material_textures(
                    vfs, &materials,
                ))
        {
            surface.textures = textures;
        }
        Ok(out)
    }

    /// Bones across every shape, and how many of them found a skeleton bone.
    #[must_use]
    pub fn bone_totals(&self) -> (usize, usize) {
        let total = self.surfaces.iter().map(|s| s.map.len()).sum();
        let matched = self.surfaces.iter().map(|s| s.map.matched()).sum();
        (total, matched)
    }
}

fn authored_tint(mesh: &soe_nif::Mesh) -> Option<[f32; 3]> {
    match &mesh.material.extra {
        soe_nif::blocks::ShaderExtra::SkinTint(color)
        | soe_nif::blocks::ShaderExtra::HairTint(color) => Some(*color),
        _ => None,
    }
}

pub(crate) fn apply_dynamic_positions(
    shape: &soe_nif::blocks::BsTriShape,
    positions: &mut Vec<[f32; 3]>,
) -> Result<(), String> {
    if shape.dynamic_positions.is_empty() {
        return Ok(());
    }
    if shape.dynamic_positions.len() != positions.len() {
        return Err("dynamic head positions do not match the vertex buffer".into());
    }
    *positions = shape
        .dynamic_positions
        .iter()
        .map(|v| [v[0], v[1], v[2]])
        .collect();
    Ok(())
}

fn expression_shapes(
    vfs: &Vfs,
    mesh_path: &str,
    mesh: &soe_nif::Mesh,
    base: &SurfaceArrays,
    mouth_targets: &[String],
) -> Result<Vec<ExpressionShape>, ActorError> {
    let is_head = mesh_path.to_ascii_lowercase().contains("head")
        || mesh.name.to_ascii_lowercase().contains("head");
    if !is_head && mouth_targets.is_empty() {
        return Ok(Vec::new());
    }
    let tri_path = std::path::Path::new(mesh_path).with_extension("tri");
    let tri_path = tri_path.to_string_lossy().replace('\\', "/");
    let Ok(bytes) = vfs.read(&tri_path) else {
        return Ok(Vec::new());
    };
    let tri = soe_nif::parse_head_tri(&bytes).map_err(|source| ActorError::Nif {
        path: tri_path,
        source,
    })?;
    if tri.vertices().len() != mesh.positions.len() {
        return Ok(Vec::new());
    }
    let mut shapes = Vec::new();
    for morph in tri.morphs() {
        let name = morph.name().trim_end_matches('\0');
        if crate::facial::expression_name_for_target(name).is_none()
            && !crate::facial::is_mouth_target(name)
            && !mouth_targets.iter().any(|target| target == name)
        {
            continue;
        }
        let positions = tri
            .apply_morph(&mesh.positions, morph.name(), 1.0)
            .map_err(|source| ActorError::Nif {
                path: mesh_path.to_owned(),
                source,
            })?;
        let mut morphed = mesh.clone();
        morphed.positions = positions;
        morphed.normals.clear();
        morphed.tangents.clear();
        let target = surface_of(&morphed).map_err(|source| ActorError::Surface {
            path: mesh_path.to_owned(),
            source,
        })?;
        shapes.push(ExpressionShape {
            name: name.to_owned(),
            positions: target
                .positions
                .iter()
                .zip(&base.positions)
                .map(|(target, base)| {
                    [
                        target[0] - base[0],
                        target[1] - base[1],
                        target[2] - base[2],
                    ]
                })
                .collect(),
            normals: target
                .normals
                .iter()
                .zip(&base.normals)
                .map(|(target, base)| {
                    [
                        target[0] - base[0],
                        target[1] - base[1],
                        target[2] - base[2],
                    ]
                })
                .collect(),
        });
    }
    Ok(shapes)
}

#[derive(Clone)]
struct NifRigNode {
    parent: Option<String>,
    transform: NifTransform,
}

fn extend_skeleton(
    skeleton: &mut Skeleton,
    file: &soe_nif::NifFile,
    required: &std::collections::BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut nodes = std::collections::BTreeMap::new();
    let mut visited = std::collections::BTreeSet::new();
    for root in &file.roots {
        collect_rig_nodes(file, *root, None, &mut visited, &mut nodes);
    }
    let mut missing = Vec::new();
    for name in required {
        if ensure_rig_node(skeleton, &nodes, name).is_err() {
            missing.push(name.clone());
        }
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

fn collect_rig_nodes(
    file: &soe_nif::NifFile,
    reference: soe_nif::BlockRef,
    parent: Option<String>,
    visited: &mut std::collections::BTreeSet<usize>,
    nodes: &mut std::collections::BTreeMap<String, NifRigNode>,
) {
    use soe_nif::blocks::Block;
    let Some(index) = reference.get() else {
        return;
    };
    if !visited.insert(index) {
        return;
    }
    let Some(Block::Node(node)) = file.blocks.get(index) else {
        return;
    };
    let name = node.av.name().to_string();
    if !name.is_empty() {
        nodes.entry(name.clone()).or_insert_with(|| NifRigNode {
            parent: parent.clone(),
            transform: node.av.transform,
        });
    }
    let parent = (!name.is_empty()).then_some(name).or(parent);
    for child in &node.children {
        collect_rig_nodes(file, *child, parent.clone(), visited, nodes);
    }
}

fn ensure_rig_node(
    skeleton: &mut Skeleton,
    nodes: &std::collections::BTreeMap<String, NifRigNode>,
    name: &str,
) -> Result<usize, ()> {
    if let Some(index) = skeleton
        .bones
        .iter()
        .position(|bone| bone.name.eq_ignore_ascii_case(name))
    {
        return Ok(index);
    }
    let node = nodes
        .iter()
        .find_map(|(node_name, node)| node_name.eq_ignore_ascii_case(name).then_some(node))
        .ok_or(())?;
    let parent = if let Some(parent) = &node.parent {
        Some(i16::try_from(ensure_rig_node(skeleton, nodes, parent)?).map_err(|_| ())?)
    } else {
        None
    };
    let index = skeleton.bones.len();
    skeleton.bones.push(HkxBone {
        name: name.to_string(),
        lock_translation: false,
    });
    skeleton.parent_indices.push(parent.unwrap_or(-1));
    skeleton
        .reference_pose
        .push(nif_transform_to_hkx(&node.transform));
    Ok(index)
}

fn nif_transform_to_hkx(transform: &NifTransform) -> soe_hkx::QsTransform {
    let m = transform.rotation.rows;
    let trace = m[0][0] + m[1][1] + m[2][2];
    let (x, y, z, w) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
            0.25 * s,
        )
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        (
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[2][1] - m[1][2]) / s,
        )
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        (
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
            (m[0][2] - m[2][0]) / s,
        )
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        (
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
            (m[1][0] - m[0][1]) / s,
        )
    };
    soe_hkx::QsTransform {
        translation: [
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
            0.0,
        ],
        rotation: [x, y, z, w],
        scale: [transform.scale, transform.scale, transform.scale, 0.0],
    }
}

/// Resolved character metadata before any skeleton, graph or clip preparation.
struct RaceCharacter {
    skeleton_model_path: String,
    project_path: String,
    character: CharacterData,
    skeleton_path: String,
    behavior_path: String,
}

fn load_race_character(
    vfs: &Vfs,
    race: &soe_esm::Record,
    female: bool,
) -> Result<RaceCharacter, ActorError> {
    let race_id = soe_esm::GlobalFormId {
        file: 0,
        object: race.form_id,
    };
    let selected = soe_game::race::AnimationModels::from_record(race)?.select(female);
    let skeleton_model_path = selected
        .skeleton
        .as_deref()
        .and_then(nonempty)
        .map(asset_path)
        .ok_or(ActorError::MissingRaceModel {
            race: race_id,
            asset: "skeleton",
        })?;
    let behavior_path = selected
        .behavior
        .as_deref()
        .and_then(nonempty)
        .map(asset_path)
        .ok_or(ActorError::MissingRaceModel {
            race: race_id,
            asset: "behavior",
        })?;
    let project_path = behavior_path;
    let bytes = read(vfs, &project_path)?;
    let project_packfile = packfile(&bytes, &project_path)?;
    let project =
        ProjectData::from_packfile(&project_packfile).map_err(|error| ActorError::Project {
            path: project_path.clone(),
            reason: error.to_string(),
        })?;
    let root = actor_root(&project_path);
    let character_name =
        project
            .character_filenames
            .first()
            .ok_or_else(|| ActorError::Project {
                path: project_path.clone(),
                reason: "project names no character data".into(),
            })?;
    let character_path = join_project_path(&root, &project.character_path, character_name);
    let bytes = read(vfs, &character_path)?;
    let character_packfile = packfile(&bytes, &character_path)?;
    let character =
        CharacterData::from_packfile(&character_packfile).map_err(|error| ActorError::Project {
            path: character_path.clone(),
            reason: error.to_string(),
        })?;
    let skeleton_name = nonempty(&character.rig_name).ok_or_else(|| ActorError::Project {
        path: character_path.clone(),
        reason: "character data names no skeleton rig".into(),
    })?;
    let behavior_name =
        nonempty(&character.behavior_filename).ok_or_else(|| ActorError::Project {
            path: character_path.clone(),
            reason: "character data names no behavior graph".into(),
        })?;
    let skeleton_path = join_project_path(&root, &project.character_path, skeleton_name);
    let behavior_path = join_project_path(&root, &project.behavior_path, behavior_name);
    Ok(RaceCharacter {
        skeleton_model_path,
        project_path,
        character,
        skeleton_path,
        behavior_path,
    })
}

fn actor_root(behavior_path: &str) -> String {
    if let Some(at) = behavior_path.to_ascii_lowercase().rfind("/behaviors/") {
        behavior_path[..=at].to_string()
    } else {
        behavior_path.rfind('/').map_or_else(
            || CHARACTER_ROOT.to_string(),
            |at| behavior_path[..=at].to_string(),
        )
    }
}

fn asset_path(name: &str) -> String {
    let path = name.replace('\\', "/");
    let path = path.trim_start_matches('/');
    if path.to_ascii_lowercase().starts_with("meshes/") {
        path.to_string()
    } else if path.to_ascii_lowercase().starts_with("actors/") {
        format!("meshes/{path}")
    } else {
        format!("{CHARACTER_ROOT}{path}")
    }
}

fn nonempty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn rooted_asset_path(root: &str, name: &str) -> String {
    let normalized = name.replace('\\', "/");
    if normalized.to_ascii_lowercase().starts_with("meshes/") {
        normalized
    } else if normalized.to_ascii_lowercase().starts_with("actors/") {
        format!("meshes/{normalized}")
    } else {
        format!("{root}{normalized}")
    }
}

fn join_project_path(root: &str, prefix: &str, name: &str) -> String {
    let prefix = prefix.replace('\\', "/");
    let name = name.replace('\\', "/");
    if name.to_ascii_lowercase().starts_with("meshes/")
        || name.to_ascii_lowercase().starts_with("actors/")
    {
        return rooted_asset_path(root, &name);
    }
    let relative = if prefix.is_empty() {
        name
    } else {
        format!("{}/{name}", prefix.trim_matches('/'))
    };
    rooted_asset_path(root, &relative)
}

fn behavior_reference_path(behavior_path: &str, name: &str) -> String {
    rooted_asset_path(&actor_root(behavior_path), name)
}

/// Omit triangles belonging to body slots covered by another addon. Part IDs
/// outside Skyrim's 30 to 61 equipment slots belong to other dismember states.
fn visible_indices(
    file: &soe_nif::NifFile,
    mesh: &soe_nif::mesh::Mesh,
    hidden_slots: u32,
) -> Result<Option<Vec<i32>>, String> {
    use soe_nif::blocks::Block;
    if hidden_slots == 0 {
        return Ok(None);
    }
    let skin = match file.blocks.get(mesh.block) {
        Some(Block::BsTriShape(shape)) => shape.skin,
        Some(Block::TriShape(shape)) => shape.skin,
        _ => return Ok(None),
    };
    let Some(Block::SkinInstance(instance)) = file.get(skin) else {
        return Ok(None);
    };
    if instance.body_parts.is_empty() {
        return Ok(None);
    }
    let Some(Block::SkinPartition(partition)) = file.get(instance.partition) else {
        return Err("dismember instance has no partition block".into());
    };
    partition_indices(
        &instance.body_parts,
        &partition.partitions,
        hidden_slots,
        mesh.positions.len(),
    )
    .map(Some)
}

fn partition_indices(
    body_parts: &[soe_nif::blocks::BodyPart],
    partitions: &[soe_nif::blocks::Partition],
    hidden_slots: u32,
    vertices: usize,
) -> Result<Vec<i32>, String> {
    if body_parts.len() != partitions.len() {
        return Err("body-part and partition counts differ".into());
    }
    let mut out = Vec::new();
    for (body, partition) in body_parts.iter().zip(partitions) {
        if (30..62).contains(&body.part) && hidden_slots & (1 << (body.part - 30)) != 0 {
            continue;
        }
        let direct = !partition.triangles_copy.is_empty();
        let triangles = if direct {
            &partition.triangles_copy
        } else {
            &partition.triangles
        };
        for triangle in triangles {
            let mut mapped = [0; 3];
            for (index, &vertex) in triangle.iter().enumerate() {
                let vertex = if direct {
                    vertex
                } else {
                    *partition
                        .vertex_map
                        .get(usize::from(vertex))
                        .ok_or("partition triangle exceeds its vertex map")?
                };
                if usize::from(vertex) >= vertices {
                    return Err("partition triangle exceeds the shape vertices".into());
                }
                mapped[index] = i32::from(vertex);
            }
            // Match build_surface's default Godot winding conversion.
            mapped.swap(1, 2);
            out.extend(mapped);
        }
    }
    Ok(out)
}

/// How many body part partitions a shape is split into, 0 when it has none.
fn partition_count(file: &soe_nif::NifFile, mesh: &soe_nif::mesh::Mesh) -> usize {
    use soe_nif::blocks::Block;

    let Some(Block::BsTriShape(shape)) = file.blocks.get(mesh.block) else {
        return 0;
    };
    let Some(Block::SkinInstance(si)) = file.get(shape.skin) else {
        return 0;
    };
    match file.get(si.partition) {
        Some(Block::SkinPartition(p)) => p.partitions.len(),
        _ => 0,
    }
}

/// Per-vertex bone indices resolved against the shape's own bone list, flat,
/// four per vertex, for a Special Edition skinned shape.
///
/// This works around a defect in `crates/nif`, reported rather than fixed here
/// because another fork owns that crate. A `NiSkinPartition` splits a shape
/// into body parts, and a packed vertex's four bone indices address the
/// **partition's** short bone list, not the skin instance's. The reader's
/// legacy `NiTriShape` path maps them through `Partition::bones` and says so in
/// its doc comment, but the `BSTriShape` path that Special Edition uses copies
/// them straight off the vertex. The result renders: vertices in the first
/// partition land on plausible bones and every later body part, arms and head
/// among them, is weighted to whatever bone happens to sit at that index.
///
/// `None` when the shape is not laid out that way, leaving the reader's
/// indices alone.
///
/// This reads the partition blocks directly rather than `mesh.bone_indices`,
/// which is why it was correct while the mesh reader was not. The reader was
/// fixed on 2026-09-18, measured over 400 actor meshes where 564 of 846
/// partitions carry a reordering bone list, so the two now agree and this
/// could be dropped in favour of `mesh.bone_indices`. Check they match on the
/// same meshes before removing it, since only this one has ever been looked at
/// on screen.
fn partition_bone_indices(file: &soe_nif::NifFile, mesh: &soe_nif::mesh::Mesh) -> Option<Vec<i32>> {
    use soe_nif::blocks::Block;

    let Some(Block::BsTriShape(shape)) = file.blocks.get(mesh.block) else {
        return None;
    };
    let Some(Block::SkinInstance(si)) = file.get(shape.skin) else {
        return None;
    };
    let Some(Block::SkinPartition(part_block)) = file.get(si.partition) else {
        return None;
    };
    if part_block.vertices.is_empty() || part_block.partitions.is_empty() {
        return None;
    }
    let vertices = mesh.positions.len();
    let mut out = vec![0i32; vertices * 4];
    let mut seen = vec![false; vertices];
    let mut claim = |at: usize, part: &soe_nif::blocks::Partition| {
        if at >= vertices || seen[at] {
            return;
        }
        let Some(vertex) = part_block.vertices.get(at) else {
            return;
        };
        seen[at] = true;
        for k in 0..4 {
            let local = vertex.bone_indices[k] as usize;
            let bone = part.bones.get(local).copied().unwrap_or(0);
            out[at * 4 + k] = i32::from(bone);
        }
    };
    for part in &part_block.partitions {
        // Which vertices a partition owns is said two ways, and Special
        // Edition shapes use either. `vertex_map` lists them directly. With a
        // shared vertex buffer that map can be empty, and then the only
        // record is the partition's own triangles in shape vertex indices.
        if part.vertex_map.is_empty() {
            for tri in &part.triangles_copy {
                for &v in tri {
                    claim(v as usize, part);
                }
            }
        } else {
            for &global in &part.vertex_map {
                claim(global as usize, part);
            }
        }
    }
    // A vertex no partition claimed keeps whatever the reader gave it, which
    // is the status quo rather than a new wrong answer.
    let unclaimed = seen.iter().filter(|v| !**v).count();
    if unclaimed == vertices {
        return None;
    }
    for (at, claimed) in seen.iter().enumerate() {
        if !claimed {
            for k in 0..4 {
                out[at * 4 + k] = i32::from(mesh.bone_indices[at][k]);
            }
        }
    }
    Some(out)
}

/// One `NIF` shape as a Godot-ready surface, skin data included.
///
/// A skinned shape's vertices are already in the skin's bind space, so the
/// placement transform is the identity here: the skinning matrices carry the
/// character's pose instead. Everything else is the same conversion the
/// renderer does for a static, so this reuses it rather than repeating it.
fn surface_of(mesh: &soe_nif::mesh::Mesh) -> Result<SurfaceArrays, crate::mesh::MeshError> {
    let data = mesh_data_from(mesh, &NifTransform::IDENTITY);
    build_surface(&data, MeshOptions::default())
}

/// Rigid biped models use their authored shape transform below an attachment bone.
/// A single weight lets the ordinary server skeleton path carry that transform.
fn rigid_surface(
    mesh: &soe_nif::Mesh,
    bone: &str,
    skeleton: &Skeleton,
) -> Result<ActorSurface, crate::mesh::MeshError> {
    let data = mesh_data_from(mesh, &mesh.transform);
    let mut arrays = build_surface(&data, MeshOptions::default())?;
    arrays.bones = vec![0; arrays.positions.len() * 4];
    arrays.weights = [1.0, 0.0, 0.0, 0.0].repeat(arrays.positions.len());
    let skin = Skin {
        skeleton_root: soe_nif::types::BlockRef::NONE,
        skeleton_root_name: String::new(),
        transform: NifTransform::IDENTITY,
        bones: vec![soe_nif::mesh::Bone {
            name: bone.to_owned(),
            node: soe_nif::types::BlockRef::NONE,
            inverse_bind: NifTransform::IDENTITY,
            bound: mesh.bound,
        }],
        body_parts: Vec::new(),
    };
    let map = BoneMap::build(&skin.bones, skeleton);
    Ok(ActorSurface {
        name: mesh.name.clone(),
        head_part_type: None,
        face_tint: None,
        face_detail: None,
        tint_color: authored_tint(mesh),
        arrays,
        expression_shapes: Vec::new(),
        textures: SurfaceTextures::default(),
        skin,
        map,
        node_offset: [0.0; 3],
        partitions: 0,
        remapped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn first_person_load_does_not_prepare_third_person_assets() {
        use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
        use soe_hkx::writer::PackfileBuilder;

        fn variant(builder: &mut PackfileBuilder, class: &str, size: usize) -> u32 {
            let root = builder.object("hkRootLevelContainer", 16);
            let object = builder.object(class, size);
            let entry = builder.array_structs(root, 0, 24, 1);
            builder.string(entry, 0, class);
            builder.string(entry, 8, class);
            builder.ptr(entry, 16, object);
            object
        }

        fn character(rig: &str, behavior: &str) -> Vec<u8> {
            let mut builder = PackfileBuilder::new();
            let character = variant(&mut builder, "hkbCharacterData", 176);
            let strings = builder.object("hkbCharacterStringData", 192);
            builder.ptr(character, 152, strings);
            builder.string(strings, 168, rig);
            builder.string(strings, 184, behavior);
            builder.finish()
        }

        let directory = tempfile::tempdir().unwrap();
        let write = |path: &str, bytes: Vec<u8>| {
            let path = directory.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        };
        let mut builder = PackfileBuilder::new();
        let project = variant(&mut builder, "hkbProjectData", 48);
        let strings = builder.object("hkbProjectStringData", 112);
        builder.ptr(project, 32, strings);
        builder.array_strings(strings, 48, &["third.hkx"]);
        builder.string(strings, 88, "behaviors/");
        builder.string(strings, 96, "characters/");
        write("meshes/actors/fixture/project.hkx", builder.finish());
        write(
            "meshes/actors/fixture/characters/third.hkx",
            character("missing-third-skeleton.hkx", "missing-third-graph.hkx"),
        );
        write(
            "meshes/actors/fixture/_1stperson/characters/firstperson.hkx",
            character("skeleton.hkx", "first.hkx"),
        );
        let mut builder = PackfileBuilder::new();
        let container = variant(&mut builder, "hkaAnimationContainer", 96);
        let skeleton = builder.object("hkaSkeleton", 120);
        builder.string(skeleton, 16, "Root");
        builder.array_i16(skeleton, 24, &[-1]);
        let bone = builder.array_structs(skeleton, 40, 16, 1);
        builder.string(bone, 0, "Root");
        builder.array_qs_transform(skeleton, 56, &[soe_hkx::QsTransform::IDENTITY]);
        builder.array_ptrs(container, 16, &[skeleton]);
        write(
            "meshes/actors/fixture/_1stperson/skeleton.hkx",
            builder.finish(),
        );
        let mut builder = PackfileBuilder::new();
        let graph = variant(&mut builder, "hkbBehaviorGraph", 304);
        let data = builder.object("hkbBehaviorGraphData", 128);
        let strings = builder.object("hkbBehaviorGraphStringData", 80);
        builder.ptr(graph, 136, data);
        builder.ptr(data, 120, strings);
        let clip = builder.object("hkbClipGenerator", 272);
        builder.string(clip, 56, "FirstPerson");
        builder.string(clip, 72, "missing-first-idle.hkx");
        builder.set_f32(clip, 100, 1.0);
        builder.ptr(graph, 128, clip);
        write(
            "meshes/actors/fixture/_1stperson/first.hkx",
            builder.finish(),
        );

        let mut plugin = Plugin::new(FileHeader::default());
        plugin.records = vec![
            Record::new(Tag(*b"RACE"), 0x100)
                .zsub(b"ANAM", "Actors/Fixture/third.nif")
                .sub(b"NAM3", [])
                .zsub(b"MODL", "Actors/Fixture/project.hkx"),
            soe_game::npc::build_record(
                7,
                "FixturePlayer",
                "Fixture player",
                soe_game::npc::Config::default(),
                soe_game::npc::Stats::default(),
            )
            .sub(b"RNAM", 0x100_u32.to_le_bytes()),
            Record::new(Tag(*b"ACHR"), 0x14).sub(b"NAME", 7_u32.to_le_bytes()),
        ];
        let mut order = LoadOrder::new();
        order.add("fixture.esm", plugin).unwrap();
        let mut world = soe_game::World::new(order);
        let actor = world.player();
        let mut vfs = Vfs::new();
        vfs.add_loose_root(directory.path()).unwrap();
        let first = ActorAssets::load_player_first_person(&mut world, actor, &vfs).unwrap();
        assert_eq!(first.skeleton.bones.len(), 1);
        assert_eq!(
            first.behavior_path,
            "meshes/Actors/Fixture/_1stperson/first.hkx"
        );
        assert!(!vfs.exists("meshes/actors/fixture/characters/missing-third-skeleton.hkx"));
        assert!(!vfs.exists("meshes/actors/fixture/behaviors/missing-third-graph.hkx"));
    }

    #[test]
    fn installed_adult_face_pixels_and_animated_normals() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
        let source = crate::data_source::DataSource::open(Path::new(&data)).unwrap();
        let world = soe_game::World::from_shared_order(source.shared_order());
        let actors: Vec<_> = ["Hulda", "Belethor", "Ysolda", "Braith"]
            .into_iter()
            .map(|name| {
                let base = world.form(name).unwrap();
                let actor = world.references_of_base(base, 1)[0];
                format!("{name}:{}:{}", actor.file, actor.object)
            })
            .collect();
        drop(world);
        drop(source);
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = workspace.join("target/adult-face-regression");
        std::fs::create_dir_all(&directory).unwrap();
        let output = std::process::Command::new(binary)
            .arg("--path")
            .arg(workspace.join("godot"))
            .args(["--resolution", "640x360", "--script", "adult_face_test.gd"])
            .env("SKYRIM_DATA", data)
            .env("DISPLAY", ":99")
            .env("SOE_FACE_ACTORS", actors.join(","))
            .env("SOE_CHARACTER_OUTPUT", &directory)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(directory.join("test.log"), &log).unwrap();
        assert!(
            output.status.success()
                && log.contains("RESULT: PASS adult face")
                && !log.contains("SCRIPT ERROR"),
            "{log}"
        );
        println!("{log}");
    }

    #[test]
    fn installed_first_person_camera_attachment_and_race_height() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = workspace.join("target/player-view-regression");
        std::fs::create_dir_all(&directory).unwrap();
        let output = std::process::Command::new(binary)
            .arg("--headless")
            .arg("--path")
            .arg(workspace.join("godot"))
            .args(["--script", "actor_view_player_test.gd"])
            .env("SKYRIM_DATA", data)
            .env("DISPLAY", ":99")
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(directory.join("test.log"), &log).unwrap();
        assert!(
            output.status.success()
                && log.contains("RESULT: PASS player view")
                && !log.contains("SCRIPT ERROR"),
            "{log}"
        );
        println!("{log}");
    }

    #[test]
    fn installed_actor_view_source_survey() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(Path::new(&data)).unwrap();
        let mut world = soe_game::World::from_shared_order(source.shared_order());
        for name in ["Hulda", "Belethor", "Ysolda", "Braith"] {
            let base = world.form(name).unwrap();
            let actor = world.references_of_base(base, 1)[0];
            let appearance = world.actor_appearance(actor).unwrap();
            println!("ACTOR {name} actor={actor} appearance={appearance:?}");
            for (owner, tag) in [
                (base, soe_esm::Tag(*b"FTST")),
                (appearance.race.unwrap(), soe_esm::Tag(*b"DFTM")),
                (appearance.race.unwrap(), soe_esm::Tag(*b"DFTF")),
            ] {
                let texture_set = world.record(owner).unwrap().find(tag).map(|bytes| {
                    let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                    world.resolve_from(owner, raw).unwrap()
                });
                println!("FACE_TEXTURE {name} field={tag:?} set={texture_set:?}");
                if let Some(texture_set) = texture_set {
                    let record = world.record(texture_set).unwrap();
                    println!("TXST {name} {:?}", record.subrecords);
                }
            }
            let face = appearance.facegen.as_ref().unwrap();
            let path = format!(
                "meshes/actors/character/facegendata/facegeom/{}/{:08X}.nif",
                face.plugin, face.object
            );
            let bytes = read(source.vfs(), &path).unwrap();
            let file = soe_nif::parse(&bytes).unwrap();
            let head_name = &appearance
                .head_parts
                .iter()
                .find(|part| part.part_type == 1)
                .unwrap()
                .name;
            let meshes = file.meshes().unwrap();
            let head = meshes
                .iter()
                .find(|mesh| mesh.name.eq_ignore_ascii_case(head_name))
                .unwrap();
            assert_eq!(
                head.material.kind,
                soe_nif::mesh::MaterialKind::Lighting(soe_nif::blocks::ShaderType::FaceTint)
            );
            println!(
                "FACEGEOM {name} path={path} kind={:?} textures={:?}",
                head.material.kind, head.material.textures
            );
            let assets = ActorAssets::load_for_actor(&mut world, actor, source.vfs()).unwrap();
            for surface in assets
                .surfaces
                .iter()
                .filter(|s| s.head_part_type == Some(1))
            {
                assert!(surface.face_tint.is_some(), "{name} has exported tint");
                assert!(surface.face_detail.is_some(), "{name} has FaceGen detail");
                assert!(
                    surface.textures.diffuse.is_some(),
                    "{name} has face diffuse"
                );
                assert!(surface.textures.normal.is_some(), "{name} has face normal");
                assert_eq!(
                    surface.arrays.material.shader_flags_1
                        & soe_nif::blocks::flags1::MODEL_SPACE_NORMALS
                        != 0,
                    name != "Braith",
                    "adult model-space normals differ from the child's tangent-space map"
                );
                println!("HEAD {name} name={} material={:?} tint={:?} detail={:?} constant={:?} diffuse={} normal={}", surface.name, surface.arrays.material, surface.face_tint.as_ref().map(|(p,_)|p), surface.face_detail.as_ref().map(|(p,_)|p), surface.tint_color, surface.textures.diffuse.is_some(), surface.textures.normal.is_some());
            }
        }
        let player = world.player();
        for assets in [
            ActorAssets::load_for_actor(&mut world, player, source.vfs()).unwrap(),
            ActorAssets::load_player_first_person(&mut world, player, source.vfs()).unwrap(),
        ] {
            let model = assets
                .skeleton
                .to_model_space(&assets.skeleton.reference_pose)
                .unwrap();
            println!("PLAYER skeleton={}", assets.skeleton_path);
            for (bone, pose) in assets.skeleton.bones.iter().zip(model).filter(|(bone, _)| {
                bone.name.contains("Head") || bone.name.to_ascii_lowercase().contains("camera")
            }) {
                println!("BONE {} {pose:?}", bone.name);
            }
        }
    }

    #[test]
    fn installed_face_pixels_and_first_person_camera_visibility() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = workspace.join("target/character-regression");
        std::fs::create_dir_all(&directory).unwrap();
        let output = std::process::Command::new(binary)
            .arg("--path")
            .arg(workspace.join("godot"))
            .args([
                "--resolution",
                "640x360",
                "--script",
                "character_regression_test.gd",
            ])
            .env("SKYRIM_DATA", data)
            .env("DISPLAY", ":99")
            .env("SOE_CHARACTER_OUTPUT", &directory)
            .env("SOE_GODOT_TIMEOUT", "180")
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::write(directory.join("test.log"), &log).unwrap();
        assert!(
            output.status.success()
                && log.contains("RESULT: PASS character regression")
                && !log.contains("SCRIPT ERROR"),
            "{log}"
        );
        println!("{log}");
    }

    #[test]
    fn installed_first_person_excludes_all_head_parts() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(Path::new(&data)).unwrap();
        let mut world = soe_game::World::from_shared_order(source.shared_order());
        let player = world.player();
        let assets =
            ActorAssets::load_player_first_person(&mut world, player, source.vfs()).unwrap();
        let heads: Vec<_> = assets
            .surfaces
            .iter()
            .filter(|s| s.head_part_type.is_some())
            .map(|s| &s.name)
            .collect();
        assert!(heads.is_empty(), "first-person head parts: {heads:?}");
        assert!(!assets.surfaces.is_empty(), "first-person arms remain");
        let update = AppearanceCache::default()
            .prepare_first_person(&mut world, player, source.vfs(), &assets.skeleton)
            .unwrap()
            .unwrap();
        assert!(
            update
                .surfaces
                .iter()
                .all(|surface| surface.head_part_type.is_none()),
            "appearance refresh also excludes head parts"
        );
    }

    #[test]
    fn installed_npc_clip_plays_for_authored_duration() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(Path::new(&data)).unwrap();
        let mut game = source.localized_game().unwrap();
        let cell = game.form("WhiterunBanneredMare").unwrap();
        game.change_cell(Some(cell));
        let contacts = game.world_mut().contact_actors();
        let hulda = contacts
            .into_iter()
            .find(|actor| game.world().name_of(actor.actor).as_deref() == Some("Hulda"))
            .unwrap()
            .actor;
        let appearance = game.world_mut().actor_appearance(hulda).unwrap();
        let assets =
            ActorAssets::load_for_actor(&mut game.world_mut(), hulda, source.vfs()).unwrap();
        let mut vm = assets.npc_interpreter(appearance.female);
        vm.update(0.0);
        let active = vm.active_clips();
        let mut failures = Vec::new();
        let mut checked = 0;
        for (id, _) in active {
            let soe_hkx::behavior::NodeKind::Clip(generator) = &assets.graph.node(id).kind else {
                continue;
            };
            if generator.mode != 1
                || generator.playback_speed != 1.0
                || generator.enforced_duration > 0.0
            {
                continue;
            }
            let duration = assets
                .clips
                .get(&generator.animation_name)
                .unwrap()
                .duration();
            let mut vm = assets.npc_interpreter(appearance.female);
            vm.update(0.0);
            let initial = vm.clip_time(id).unwrap();
            let mut previous = initial;
            let dt = 1.0 / 120.0;
            let mut played = None;
            for frame in 1..=((duration * 1.1 / dt).ceil() as u32 + 1) {
                vm.update(dt);
                let Some(now) = vm.clip_time(id) else { break };
                if now < previous {
                    played = Some(frame as f32 * dt + initial);
                    break;
                }
                previous = now;
            }
            println!(
                "clip {} duration={duration} played={played:?} speed={} crops=({}, {})",
                generator.animation_name,
                generator.playback_speed,
                generator.crop_start_amount_local_time,
                generator.crop_end_amount_local_time
            );
            checked += 1;
            let expected = duration
                - generator.crop_start_amount_local_time
                - generator.crop_end_amount_local_time;
            if played.is_none_or(|played| (played - expected).abs() / expected > 0.05) {
                failures.push((generator.animation_name.clone(), expected, played));
            }
        }
        assert!(checked > 0, "NPC graph has authored idle clips");
        assert!(
            failures.is_empty(),
            "clip durations outside 5 percent: {failures:?}"
        );
    }

    #[test]
    fn r4_sneak_extracted_graphs_play_idle_walk_run_and_exit() {
        let Some(directory) = std::env::var_os("SOE_HKX_DIR") else {
            return;
        };
        let mut vfs = Vfs::new();
        vfs.add_loose_root(std::path::Path::new(&directory))
            .unwrap();
        for (root, character) in [
            (CHARACTER_ROOT.to_owned(), "characters/defaultmale.hkx"),
            (
                format!("{CHARACTER_ROOT}_1stperson/"),
                "characters/firstperson.hkx",
            ),
        ] {
            let path = format!("{root}{character}");
            let bytes = vfs.read(&path).unwrap();
            let character =
                CharacterData::from_packfile(&Packfile::parse(&bytes).unwrap()).unwrap();
            let bytes = vfs
                .read(&format!("{root}{}", character.rig_name.replace('\\', "/")))
                .unwrap();
            let skeleton = AnimationContainer::from_packfile(&Packfile::parse(&bytes).unwrap())
                .unwrap()
                .skeletons
                .remove(0);
            let (graph, unresolved) = ActorAssets::load_graph(
                &vfs,
                &format!("{root}{}", character.behavior_filename.replace('\\', "/")),
            )
            .unwrap();
            assert!(unresolved.is_empty(), "unresolved graphs: {unresolved:?}");
            let (library, _) = ActorAssets::load_clips(&vfs, &graph, &root);
            for npc in [false, true] {
                if npc && root.contains("_1stperson") {
                    continue;
                }
                let mut vm = soe_hkx::interp::Interpreter::new(
                    &graph,
                    &library,
                    skeleton.reference_pose.clone(),
                );
                for (name, word) in character.character_property_names.iter().zip(
                    character
                        .character_property_values
                        .as_ref()
                        .map(|values| values.word_values.as_slice())
                        .unwrap_or_default(),
                ) {
                    if let Some(index) = vm.character_properties.index(name) {
                        vm.character_properties.set_word(index, *word);
                    }
                }
                for (name, value) in [
                    ("IsNPC", f32::from(npc)),
                    ("IsPlayer", f32::from(!npc)),
                    ("iIsPlayer", f32::from(!npc)),
                ] {
                    vm.variables.set_by_name(name, value);
                }
                let active = |vm: &soe_hkx::interp::Interpreter<'_>| -> Vec<String> {
                    vm.active_clips()
                        .iter()
                        .filter_map(|(id, _)| match &graph.node(*id).kind {
                            soe_hkx::behavior::NodeKind::Clip(clip) => {
                                Some(clip.animation_name.to_ascii_lowercase())
                            }
                            _ => None,
                        })
                        .collect()
                };
                let mut posture = crate::animation_dispatch::SneakGraphState::default();
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                println!("{root} npc={npc} standing={:?}", active(&vm));
                posture.apply(&mut vm, true);
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                println!("{root} npc={npc} sneak idle={:?}", active(&vm));
                println!(
                    "sneak sync={:?}",
                    vm.variables
                        .index("iIsInSneak")
                        .and_then(|index| vm.variables.word(index))
                );
                assert_eq!(
                    vm.variables
                        .index("iIsInSneak")
                        .and_then(|index| vm.variables.word(index)),
                    Some(1)
                );
                assert!(
                    active(&vm).iter().any(|clip| clip.contains("sneak")),
                    "sneak idle absent"
                );
                vm.send_event_named("moveStart");
                for speed in [47.2, 222.0] {
                    vm.variables.set_by_name("Speed", speed);
                    for _ in 0..60 {
                        vm.update(1.0 / 60.0);
                    }
                    println!("{root} npc={npc} speed={speed} clips={:?}", active(&vm));
                    let gait = if speed < 100.0 {
                        "sneakwalk"
                    } else {
                        "sneakrun"
                    };
                    assert!(
                        active(&vm).iter().any(|clip| clip.contains(gait)),
                        "sneak locomotion absent"
                    );
                }
                posture.apply(&mut vm, false);
                vm.variables.set_by_name("Speed", 0.0);
                vm.send_event_named("moveStop");
                for _ in 0..120 {
                    vm.update(1.0 / 60.0);
                }
                println!("{root} npc={npc} exit={:?}", active(&vm));
                assert!(
                    !active(&vm).iter().any(|clip| clip.contains("sneak")),
                    "sneak persisted after exit"
                );
            }
        }
    }

    #[test]
    fn r4_sneak_installed_graphs_follow_shared_posture() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
        let mut game = source.localized_game().unwrap();
        let player = game.player();
        for first_person in [false, true] {
            let assets = if first_person {
                ActorAssets::load_player_first_person(&mut game.world_mut(), player, source.vfs())
            } else {
                ActorAssets::load_for_actor(&mut game.world_mut(), player, source.vfs())
            }
            .unwrap();
            for npc in [false, true] {
                if first_person && npc {
                    continue;
                }
                let mut vm = if npc {
                    assets.npc_interpreter(false)
                } else {
                    assets.interpreter()
                };
                let mut posture = crate::animation_dispatch::SneakGraphState::default();
                let clips = |vm: &soe_hkx::interp::Interpreter<'_>| -> Vec<String> {
                    vm.active_clips()
                        .iter()
                        .filter_map(|(id, _)| match &assets.graph.node(*id).kind {
                            soe_hkx::behavior::NodeKind::Clip(clip) => {
                                Some(clip.animation_name.to_ascii_lowercase())
                            }
                            _ => None,
                        })
                        .collect()
                };
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                posture.apply(&mut vm, true);
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                let idle = clips(&vm);
                println!("sneak first_person={first_person} npc={npc} idle={idle:?}");
                assert!(idle.iter().any(|clip| clip.contains("sneak")));
                let initial = vm.pose().to_vec();
                vm.variables.set_by_name("Speed", 47.2);
                vm.send_event_named("moveStart");
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                let walking = clips(&vm);
                println!("sneak first_person={first_person} npc={npc} walk={walking:?}");
                assert!(walking.iter().any(|clip| clip.contains("sneak")));
                assert!(walking.iter().any(|clip| clip.contains("sneakwalk")));
                assert_ne!(initial, vm.pose());
                vm.variables.set_by_name("Speed", 222.0);
                for _ in 0..60 {
                    vm.update(1.0 / 60.0);
                }
                assert!(clips(&vm).iter().any(|clip| clip.contains("sneakrun")));
                posture.apply(&mut vm, false);
                vm.variables.set_by_name("Speed", 0.0);
                vm.send_event_named("moveStop");
                for _ in 0..120 {
                    vm.update(1.0 / 60.0);
                }
                assert!(!clips(&vm).iter().any(|clip| clip.contains("sneak")));
            }
        }
    }

    #[test]
    fn hst_23_installed_hulda_head_and_idle_pose() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
        let mut game = source.localized_game().unwrap();
        let cell = game.form("WhiterunBanneredMare").unwrap();
        game.change_cell(Some(cell));
        let contacts = game.world_mut().contact_actors();
        let hulda = contacts
            .into_iter()
            .find(|actor| game.world().name_of(actor.actor).as_deref() == Some("Hulda"))
            .unwrap()
            .actor;
        let appearance = game.world_mut().actor_appearance(hulda).unwrap();
        let assets =
            ActorAssets::load_for_actor(&mut game.world_mut(), hulda, source.vfs()).unwrap();
        for part_type in [1, 2, 3] {
            let surfaces: Vec<_> = assets
                .surfaces
                .iter()
                .filter(|surface| surface.head_part_type == Some(part_type))
                .collect();
            assert!(
                !surfaces.is_empty(),
                "Hulda has head, eyes and hair geometry"
            );
            for surface in surfaces {
                assert!(
                    surface
                        .arrays
                        .positions
                        .iter()
                        .any(|vertex| vertex.iter().any(|value| value.abs() > 0.01)),
                    "{} has nonzero dynamic geometry",
                    surface.name
                );
                assert!(
                    surface.map.unmatched.is_empty(),
                    "{} binds to the race skeleton",
                    surface.name
                );
            }
        }
        let head = assets
            .surfaces
            .iter()
            .find(|surface| surface.head_part_type == Some(1))
            .unwrap();
        assert!(head.face_tint.is_some(), "Hulda's exported face tint loads");
        assert!(
            head.face_detail.is_some(),
            "Hulda's FaceGen detail map loads"
        );
        assert!(
            head.textures.diffuse.is_some(),
            "head diffuse loads independently of tint"
        );
        assert_ne!(
            head.arrays.material.shader_flags_1 & soe_nif::blocks::flags1::MODEL_SPACE_NORMALS,
            0
        );
        assert!(
            head.expression_shapes.iter().any(|shape| {
                crate::facial::is_mouth_target(&shape.name)
                    && shape
                        .positions
                        .iter()
                        .flatten()
                        .any(|delta| delta.abs() > 0.001)
            }),
            "Hulda's exported face retains the head part's mouth morph deltas"
        );
        assert_eq!(appearance.facegen.as_ref().unwrap().object, 0x13BA3);
        let mut vm = assets.npc_interpreter(appearance.female);
        vm.update(1.0 / 60.0);
        let initial = vm.pose().to_vec();
        for _ in 0..60 {
            vm.update(1.0 / 60.0);
        }
        assert_ne!(
            initial, assets.skeleton.reference_pose,
            "idle must leave the reference pose"
        );
        assert_ne!(initial, vm.pose(), "idle pose must change over one second");
        vm.variables.set_by_name("Speed", 100.0);
        assert!(vm.send_event_named("moveStart"));
        for _ in 0..60 {
            vm.update(1.0 / 60.0);
        }
        let walking = vm.active_clips().iter().any(|(id, _)| {
            matches!(&assets.graph.node(*id).kind, soe_hkx::behavior::NodeKind::Clip(clip) if clip.animation_name.to_ascii_lowercase().contains("walk"))
        });
        assert!(
            walking,
            "NPC movement reaches a walk clip through the selected graph"
        );
        vm.variables.set_by_name("Speed", 0.0);
        assert!(vm.send_event_named("moveStop"));
        for _ in 0..60 {
            vm.update(1.0 / 60.0);
        }
        assert!(vm.active_clips().iter().any(|(id, _)| {
            matches!(&assets.graph.node(*id).kind, soe_hkx::behavior::NodeKind::Clip(clip) if clip.animation_name.to_ascii_lowercase().contains("idle"))
        }), "NPC movement stop returns to idle through the graph");
    }

    #[test]
    fn hst_23_dynamic_head_positions_replace_zero_packed_vertices() {
        let shape = soe_nif::blocks::BsTriShape {
            dynamic_positions: vec![[10.0, 20.0, 30.0, 1.0], [-5.0, 4.0, 100.0, 0.0]],
            ..Default::default()
        };
        let mut positions = vec![[0.0; 3]; 2];
        apply_dynamic_positions(&shape, &mut positions).unwrap();
        assert_eq!(positions, [[10.0, 20.0, 30.0], [-5.0, 4.0, 100.0]]);
    }

    #[test]
    fn hst_23_dynamic_head_vertex_mismatch_preserves_previous_positions() {
        let shape = soe_nif::blocks::BsTriShape {
            dynamic_positions: vec![[1.0; 4]],
            ..Default::default()
        };
        let mut positions = vec![[0.0; 3]; 2];
        assert!(apply_dynamic_positions(&shape, &mut positions).is_err());
        assert_eq!(positions, [[0.0; 3]; 2]);
    }

    fn rigid_shield_fixture() -> (Skeleton, soe_nif::Mesh) {
        let skeleton = Skeleton {
            name: "test".into(),
            parent_indices: vec![-1],
            bones: vec![HkxBone {
                name: "SHIELD".into(),
                lock_translation: false,
            }],
            reference_pose: vec![soe_hkx::QsTransform::IDENTITY],
            reference_floats: Vec::new(),
            float_slots: Vec::new(),
        };
        let mesh = soe_nif::Mesh {
            name: "shield".into(),
            block: 0,
            transform: NifTransform {
                translation: soe_nif::types::Vec3::new(10.0, 20.0, 30.0),
                ..NifTransform::IDENTITY
            },
            bound: soe_nif::Bound::default(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: Vec::new(),
            tangents: Vec::new(),
            uvs: Vec::new(),
            uv_sets: Vec::new(),
            colors: Vec::new(),
            indices: vec![0, 1, 2],
            bone_weights: Vec::new(),
            bone_indices: Vec::new(),
            skin: None,
            material: soe_nif::Material::default(),
        };
        (skeleton, mesh)
    }

    #[test]
    fn uploaded_actor_images_release_with_face_detail_and_keep_geometry() {
        let (skeleton, mesh) = rigid_shield_fixture();
        let mut surface = rigid_surface(&mesh, "SHIELD", &skeleton).unwrap();
        let image = std::sync::Arc::new(crate::data_source::TextureData::Rgba8(soe_dds::Image {
            width: 1,
            height: 1,
            depth: 1,
            rgba: vec![128; 4],
        }));
        let released = std::sync::Arc::downgrade(&image);
        surface.textures.diffuse = Some(std::sync::Arc::clone(&image));
        surface.face_tint = Some(("tint.dds".into(), std::sync::Arc::clone(&image)));
        surface.face_detail = Some(("detail.dds".into(), image));
        let geometry = surface.arrays.positions.clone();
        let before = crate::skinned::skinning_transforms(
            &surface.map,
            &surface.skin,
            &skeleton.reference_pose,
        )
        .unwrap();
        surface.release_uploaded_images();
        assert!(
            released.upgrade().is_none(),
            "upload-only face detail kept the image alive"
        );
        assert_eq!(surface.arrays.positions, geometry);
        let after = crate::skinned::skinning_transforms(
            &surface.map,
            &surface.skin,
            &skeleton.reference_pose,
        )
        .unwrap();
        assert_eq!(after[0].origin, before[0].origin);
    }

    #[test]
    fn hst_23_rigid_shield_keeps_shape_transform_and_tracks_attachment_pose() {
        let (skeleton, mesh) = rigid_shield_fixture();
        let surface = rigid_surface(&mesh, "SHIELD", &skeleton).unwrap();
        assert_eq!(surface.map.matched(), 1);
        assert_eq!(surface.arrays.weights, [1.0, 0.0, 0.0, 0.0].repeat(3));
        for (actual, expected) in surface.arrays.positions[0]
            .iter()
            .zip([0.142875, 0.428625, -0.28575])
        {
            assert!((actual - expected).abs() < 0.00001);
        }
        let mut pose = skeleton.reference_pose.clone();
        let initial =
            crate::skinned::skinning_transforms(&surface.map, &surface.skin, &pose).unwrap();
        pose[0].translation[0] = 100.0;
        let moved =
            crate::skinned::skinning_transforms(&surface.map, &surface.skin, &pose).unwrap();
        assert!((moved[0].origin[0] - initial[0].origin[0] - 1.42875).abs() < 0.00001);
    }

    #[test]
    fn hst_23_installed_guard_outfit_includes_rigid_shield() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
        let game = source.localized_game().unwrap();
        let actor = game.form("GuardWhiterunImperialPostNightRef1").unwrap();
        let assets =
            ActorAssets::load_for_actor(&mut game.world_mut(), actor, source.vfs()).unwrap();
        let shields: Vec<_> = assets
            .surfaces
            .iter()
            .filter(|surface| {
                surface.skin.bones.len() == 1
                    && crate::combat::weapon_bones(soe_game::combat::Hand::Left)
                        .contains(&surface.skin.bones[0].name.as_str())
                    && surface.name.to_ascii_lowercase().contains("shield")
            })
            .collect();
        assert!(!shields.is_empty());
        assert!(shields.iter().all(|surface| surface.map.matched() == 1));
        assert!(assets.surfaces.len() >= 3);
    }

    #[test]
    fn covered_body_part_triangles_are_removed_in_both_partition_layouts() {
        use soe_nif::blocks::{BodyPart, Partition};
        let bodies = [
            BodyPart { part: 37, flags: 0 },
            BodyPart { part: 38, flags: 0 },
        ];
        let partitions = [
            Partition {
                triangles_copy: vec![[0, 1, 2]],
                ..Default::default()
            },
            Partition {
                vertex_map: vec![3, 4, 5],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            },
        ];
        assert_eq!(
            partition_indices(&bodies, &partitions, 1 << 7, 6),
            Ok(vec![3, 5, 4])
        );
        assert_eq!(
            partition_indices(&bodies, &partitions, 1 << 8, 6),
            Ok(vec![0, 2, 1])
        );
        assert_eq!(
            partition_indices(&bodies, &partitions, (1 << 7) | (1 << 8), 6),
            Ok(vec![])
        );
        assert!(partition_indices(&bodies[..1], &partitions, 1 << 7, 6).is_err());
        assert!(partition_indices(&bodies, &partitions, 1 << 7, 5).is_err());
    }

    #[test]
    fn a_havok_path_becomes_an_archive_path_under_the_character_root() {
        assert_eq!(
            rooted_asset_path(CHARACTER_ROOT, r"Animations\MT_Idle.HKX"),
            "meshes/actors/character/Animations/MT_Idle.HKX"
        );
    }

    #[test]
    fn a_path_with_no_backslashes_is_left_alone_apart_from_the_root() {
        assert_eq!(
            rooted_asset_path(CHARACTER_ROOT, "behaviors/mt_behavior.hkx"),
            "meshes/actors/character/behaviors/mt_behavior.hkx"
        );
    }
}

#[cfg(test)]
mod hst_29_paths {
    use super::{actor_root, behavior_reference_path};

    #[test]
    fn hst_29_vanilla_mixed_case_behavior_root_resolves_child_graph() {
        let master = "meshes/Actors/Character/Behaviors/0_Master.hkx";
        assert_eq!(actor_root(master), "meshes/Actors/Character/");
        assert_eq!(
            behavior_reference_path(master, r"Behaviors\mt_behavior.hkx"),
            "meshes/Actors/Character/Behaviors/mt_behavior.hkx"
        );
    }

    #[test]
    fn hst_29_behavior_root_preserves_creature_and_lowercase_paths() {
        for root in ["meshes/actors/character/", "meshes/actors/wolf/"] {
            let master = format!("{root}behaviors/master.hkx");
            assert_eq!(actor_root(&master), root);
            assert_eq!(
                behavior_reference_path(&master, r"Behaviors\idle.hkx"),
                format!("{root}Behaviors/idle.hkx")
            );
        }
    }
}

#[cfg(test)]
mod npc_motion_tests {
    use super::NpcMotion;
    use soe_game::world::RefPlacement;

    fn placement() -> RefPlacement {
        RefPlacement {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            visible: true,
        }
    }

    #[test]
    fn actor_body_heading_excludes_pitch_and_roll() {
        let mut looking = placement();
        looking.position = [17.0, 23.0, 41.0];
        looking.scale = 1.08;
        looking.rotation = [-1.2, 0.3, 0.7];
        let heading = RefPlacement {
            rotation: [0.0, 0.0, looking.rotation[2]],
            ..looking
        };
        assert_eq!(super::npc_transform(looking), super::npc_transform(heading));
    }

    #[test]
    fn invalid_previous_pose_does_not_produce_graph_inputs() {
        let current = placement();
        let mut previous = current;
        previous.rotation[2] = f32::NAN;
        assert!(NpcMotion::between(previous, current, 1.0).is_none());
    }

    #[test]
    fn finite_coordinates_with_overflowing_graph_speed_are_rejected() {
        let previous = placement();
        let mut current = previous;
        current.position[0] = f32::MAX;
        assert!(NpcMotion::between(previous, current, 0.01).is_none());
    }
}
