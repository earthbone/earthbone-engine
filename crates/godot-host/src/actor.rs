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

/// One named CK expression target in Godot surface coordinates.
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
        let character = CharacterData::from_packfile(&character_packfile).map_err(|error| {
            ActorError::Project {
                path: character_path.clone(),
                reason: error.to_string(),
            }
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
        let missing: std::collections::BTreeSet<_> = assets
            .surfaces
            .iter()
            .flat_map(|surface| surface.map.unmatched.iter().cloned())
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        let bytes = read(vfs, &assets.skeleton_model_path)?;
        let file = soe_nif::parse(&bytes).map_err(|source| ActorError::Nif {
            path: assets.skeleton_model_path.clone(),
            source,
        })?;
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
        for part in &appearance.head_parts {
            if let Some(model) = &part.model {
                surfaces.extend(Self::load_attached_surfaces(
                    vfs,
                    &asset_path(model),
                    skeleton,
                    0,
                    None,
                    mouth_targets,
                )?);
            }
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
        let meshes = file.meshes().map_err(|source| ActorError::Nif {
            path: mesh_path.to_string(),
            source,
        })?;
        let mut out = Vec::new();
        for mesh in &meshes {
            if mesh.skin.is_none() {
                if let Some(bone) = attachment {
                    out.push(rigid_surface(mesh, bone, skeleton).map_err(|source| {
                        ActorError::Surface {
                            path: mesh_path.to_owned(),
                            source,
                        }
                    })?);
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

fn actor_root(behavior_path: &str) -> String {
    if let Some(at) = behavior_path.rfind("/behaviors/") {
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

    #[test]
    fn hst_23_rigid_shield_keeps_shape_transform_and_tracks_attachment_pose() {
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
