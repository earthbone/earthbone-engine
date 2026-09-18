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

use soe_bsa::Vfs;
use soe_havok::behavior::BehaviorGraph;
use soe_havok::interp::{Clip, ClipLibrary};
use soe_havok::skeleton::{AnimationContainer, Skeleton};
use soe_havok::Packfile;
use soe_nif::mesh::Skin;
use soe_nif::types::Transform as NifTransform;

use crate::data_source::mesh_data_from;
use crate::mesh::{build_surface, MeshOptions, SurfaceArrays};
use crate::skinned::BoneMap;

/// Where the vanilla human character's files live inside the archives.
pub const CHARACTER_ROOT: &str = "meshes/actors/character/";

/// The vanilla human skeleton, relative to [`CHARACTER_ROOT`].
pub const HUMAN_SKELETON: &str = "character assets/skeleton.hkx";

/// The locomotion graph, relative to [`CHARACTER_ROOT`]. It is the graph the
/// `behavior-2` work proved out, and it names the idle and walk clips.
pub const LOCOMOTION_BEHAVIOR: &str = "behaviors/mt_behavior.hkx";

/// What went wrong assembling a character.
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
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
        source: soe_havok::Error,
    },
    #[error("parsing {path}: {source}")]
    Nif {
        path: String,
        #[source]
        source: soe_nif::Error,
    },
    #[error("{path} holds no skeleton")]
    NoSkeleton { path: String },
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
    /// The shape's own bone list, carrying the inverse bind transforms.
    pub skin: Skin,
    /// That list resolved against the skeleton.
    pub map: BoneMap,
}

/// A character's skeleton, graph, clips and skinned shapes.
pub struct ActorAssets {
    pub skeleton: Skeleton,
    pub graph: BehaviorGraph,
    pub clips: ClipLibrary,
    pub surfaces: Vec<ActorSurface>,
    /// Clip names the graph asked for that the archives do not hold.
    pub missing_clips: Vec<String>,
    /// Referenced behavior files that linking could not find.
    pub unresolved_references: Vec<String>,
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

/// A Havok path as the files write it, `Animations\MT_Idle.HKX`, turned into
/// an archive path under the character root.
fn character_path(name: &str) -> String {
    format!("{CHARACTER_ROOT}{}", name.replace('\\', "/"))
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
        let skeleton = Self::load_skeleton(vfs)?;
        let (graph, unresolved_references) = Self::load_graph(vfs)?;
        let (clips, missing_clips) = Self::load_clips(vfs, &graph);
        let surfaces = Self::load_surfaces(vfs, mesh_path, &skeleton)?;
        Ok(ActorAssets {
            skeleton,
            graph,
            clips,
            surfaces,
            missing_clips,
            unresolved_references,
        })
    }

    fn load_skeleton(vfs: &Vfs) -> Result<Skeleton, ActorError> {
        let path = format!("{CHARACTER_ROOT}{HUMAN_SKELETON}");
        let bytes = read(vfs, &path)?;
        let pf = packfile(&bytes, &path)?;
        let container =
            AnimationContainer::from_packfile(&pf).map_err(|source| ActorError::Havok {
                path: path.clone(),
                source,
            })?;
        container
            .skeletons
            .into_iter()
            .next()
            .ok_or(ActorError::NoSkeleton { path })
    }

    fn load_graph(vfs: &Vfs) -> Result<(BehaviorGraph, Vec<String>), ActorError> {
        let path = format!("{CHARACTER_ROOT}{LOCOMOTION_BEHAVIOR}");
        let bytes = read(vfs, &path)?;
        let pf = packfile(&bytes, &path)?;
        let mut graph = BehaviorGraph::from_packfile(&pf).map_err(|source| ActorError::Havok {
            path: path.clone(),
            source,
        })?;
        // A referenced behavior may itself reference another, so the walk
        // repeats. Eight rounds is what the gated havok test uses and the
        // vanilla graph settles in two. See docs/havok.md.
        graph.link_all(8, |name| {
            let sub_path = character_path(name);
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

    fn load_clips(vfs: &Vfs, graph: &BehaviorGraph) -> (ClipLibrary, Vec<String>) {
        let mut clips = ClipLibrary::new();
        let mut missing = Vec::new();
        for name in graph.clip_animation_names() {
            if clips.get(&name).is_some() {
                continue;
            }
            let path = character_path(&name);
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
        let bytes = read(vfs, mesh_path)?;
        let file = soe_nif::parse(&bytes).map_err(|source| ActorError::Nif {
            path: mesh_path.to_string(),
            source,
        })?;
        let meshes = file.meshes().map_err(|source| ActorError::Nif {
            path: mesh_path.to_string(),
            source,
        })?;
        let mut out = Vec::new();
        for mesh in &meshes {
            let Some(skin) = mesh.skin.clone() else {
                continue;
            };
            if mesh.bone_weights.is_empty() {
                continue;
            }
            let arrays = surface_of(mesh).map_err(|source| ActorError::Surface {
                path: mesh_path.to_string(),
                source,
            })?;
            let map = BoneMap::build(&skin.bones, skeleton);
            out.push(ActorSurface {
                name: mesh.name.clone(),
                arrays,
                skin,
                map,
            });
        }
        if out.is_empty() {
            return Err(ActorError::NotSkinned {
                path: mesh_path.to_string(),
            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_havok_path_becomes_an_archive_path_under_the_character_root() {
        assert_eq!(
            character_path(r"Animations\MT_Idle.HKX"),
            "meshes/actors/character/Animations/MT_Idle.HKX"
        );
    }

    #[test]
    fn a_path_with_no_backslashes_is_left_alone_apart_from_the_root() {
        assert_eq!(
            character_path("behaviors/mt_behavior.hkx"),
            "meshes/actors/character/behaviors/mt_behavior.hkx"
        );
    }
}
