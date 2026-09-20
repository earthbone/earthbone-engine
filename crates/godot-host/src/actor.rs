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
use soe_hkx::behavior::BehaviorGraph;
use soe_hkx::interp::{Clip, ClipLibrary};
use soe_hkx::skeleton::{AnimationContainer, Skeleton};
use soe_hkx::Packfile;
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
        // repeats. Eight rounds is what the gated hkx test uses and the
        // vanilla graph settles in two. See docs/hkx.md.
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
        let mut surfaces = Vec::new();
        for part in &appearance.parts {
            let Some(model) = &part.model else { continue };
            let path = model.replace('\\', "/");
            let path = if path.to_ascii_lowercase().starts_with("meshes/") {
                path
            } else {
                format!("meshes/{path}")
            };
            surfaces.extend(Self::load_visible_surfaces(
                vfs,
                &path,
                skeleton,
                part.authored_slots & !part.visible_slots,
            )?);
        }
        Ok(surfaces)
    }

    fn load_visible_surfaces(
        vfs: &Vfs,
        mesh_path: &str,
        skeleton: &Skeleton,
        hidden_slots: u32,
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
            let mut arrays = surface_of(mesh).map_err(|source| ActorError::Surface {
                path: mesh_path.to_string(),
                source,
            })?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
