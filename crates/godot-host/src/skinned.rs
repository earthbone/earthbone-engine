// SPDX-License-Identifier: GPL-3.0-or-later

//! Joining a skinned mesh to an animated skeleton.
//!
//! Two files describe the same character and neither knows about the other. A
//! `NIF` shape carries vertices weighted to bones **it** names, in its own
//! order, each with the inverse bind transform that moves a vertex from the
//! skin's bind space into that bone's space. An `hkx` skeleton carries the
//! bones the behavior graph animates, in a different order and usually more of
//! them. The only thing the two share is the bone names.
//!
//! So this module does three things: match the mesh's bone list onto the
//! skeleton's by name, combine the animated model-space pose with the mesh's
//! bind transform to get one skinning transform per mesh bone, and convert
//! that into Godot's axes and metres.
//!
//! One trap is worth knowing before reading the arithmetic. A bind transform
//! has to be transposed where a scene graph node's transform does not, which
//! is what [`Affine::from_nif_skin`] is for and why it exists beside
//! [`Affine::from_nif`]. Leaving it out flattens every limb while sparing the
//! pelvis, whose bind rotation is the identity and so is its own transpose.
//! The reasoning, and how it lines up with `nifly`, is on `from_nif_skin`.
//!
//! Everything here is plain arithmetic with no Godot types, so it is testable
//! without an engine. [`crate::godot_layer`] turns the result into
//! `RenderingServer` calls.
//!
//! On the conversion: the map `(x, y, z) -> (x, z, -y)` is a rotation with
//! determinant +1 (see [`crate::units`]), so a transform converts by
//! conjugation, `C * M * C inverse`. Because the scale factor in `C` is
//! uniform it cancels out of the basis and survives only in the origin, and
//! because conjugation is a homomorphism it does not matter whether transforms
//! are composed before or after converting. They are composed first here, so
//! the conversion runs once per bone rather than once per link in the chain.

use soe_havok::skeleton::Skeleton;
use soe_havok::QsTransform;
use soe_nif::mesh::{Bone as MeshBone, Skin};
use soe_nif::types::Transform as NifTransform;

use crate::units::METRES_PER_UNIT;

/// A 3 by 4 affine transform: a basis in row major order, and an origin.
///
/// This is Godot's `Transform3D` without the dependency, and the shape both
/// `hkQsTransform` and the `NIF` transform flatten into.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub basis: [[f32; 3]; 3],
    pub origin: [f32; 3],
}

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        origin: [0.0, 0.0, 0.0],
    };

    /// A Havok transform, with its per axis scale folded into the basis.
    #[must_use]
    pub fn from_qs(q: &QsTransform) -> Affine {
        let [x, y, z, w] = q.rotation;
        // The standard quaternion to matrix expansion.
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        let rot = [
            [1.0 - 2.0 * (yy + zz), 2.0 * (xy - wz), 2.0 * (xz + wy)],
            [2.0 * (xy + wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz - wx)],
            [2.0 * (xz - wy), 2.0 * (yz + wx), 1.0 - 2.0 * (xx + yy)],
        ];
        let mut basis = [[0.0f32; 3]; 3];
        for (r, row) in basis.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                // Column c scales by the transform's scale on axis c.
                *cell = rot[r][c] * q.scale[c];
            }
        }
        Affine {
            basis,
            origin: [q.translation[0], q.translation[1], q.translation[2]],
        }
    }

    /// A `NIF` transform, whose scale is uniform.
    #[must_use]
    pub fn from_nif(t: &NifTransform) -> Affine {
        let mut basis = t.rotation.rows;
        for row in &mut basis {
            for cell in row.iter_mut() {
                *cell *= t.scale;
            }
        }
        Affine {
            basis,
            origin: [t.translation.x, t.translation.y, t.translation.z],
        }
    }

    /// A bind transform out of `NiSkinData`, with its basis transposed.
    ///
    /// This matches `nifly`, the C++ library under PyNifly, which imports and
    /// exports Skyrim characters with working skinning. Two conventions have
    /// to be lined up to see why, and they cancel.
    ///
    /// `nif.xml` declares `Matrix33` as nine floats in OpenGL column major
    /// order, `m11 m21 m31 m12 ...`, and `crates/nif` reads it that way, so
    /// `Mat33::rows` is the matrix as written and [`Affine::from_nif`] applies
    /// it as `M * v`. That is right for a scene graph node, which the streamed
    /// cells prove every time they draw. `nifly` instead reads the same nine
    /// floats straight into memory, so its `Matrix3` holds the transpose of
    /// ours, and its `ApplyTransform` computes what we would write as
    /// `transpose(M) * v`. Applying the transpose here is therefore doing what
    /// `nifly` does, not departing from it.
    ///
    /// Leaving it out is the bind pose defect, and it hides well: the pelvis
    /// bind rotation is the identity, which is its own transpose, so the error
    /// is zero there and grows through every bone below it. See
    /// `docs/godot-host.md`.
    #[must_use]
    pub fn from_nif_skin(t: &NifTransform) -> Affine {
        let a = Affine::from_nif(t);
        let mut basis = [[0.0f32; 3]; 3];
        for (r, row) in basis.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = a.basis[c][r];
            }
        }
        Affine {
            basis,
            origin: a.origin,
        }
    }

    /// `self` then `other` applied to a point as `self * other * v`.
    #[must_use]
    pub fn then(&self, other: &Affine) -> Affine {
        let mut basis = [[0.0f32; 3]; 3];
        for (r, row) in basis.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.basis[r][k] * other.basis[k][c]).sum();
            }
        }
        let o = other.origin;
        let origin = [
            self.basis[0][0] * o[0]
                + self.basis[0][1] * o[1]
                + self.basis[0][2] * o[2]
                + self.origin[0],
            self.basis[1][0] * o[0]
                + self.basis[1][1] * o[1]
                + self.basis[1][2] * o[2]
                + self.origin[1],
            self.basis[2][0] * o[0]
                + self.basis[2][1] * o[1]
                + self.basis[2][2] * o[2]
                + self.origin[2],
        ];
        Affine { basis, origin }
    }

    /// Apply to a point.
    #[must_use]
    pub fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        [
            self.basis[0][0] * v[0]
                + self.basis[0][1] * v[1]
                + self.basis[0][2] * v[2]
                + self.origin[0],
            self.basis[1][0] * v[0]
                + self.basis[1][1] * v[1]
                + self.basis[1][2] * v[2]
                + self.origin[1],
            self.basis[2][0] * v[0]
                + self.basis[2][1] * v[1]
                + self.basis[2][2] * v[2]
                + self.origin[2],
        ]
    }

    /// The same transform expressed in Godot's axes and metres.
    ///
    /// Conjugation by the axis map, with the unit scale applied to the origin
    /// only. See the module docs for why that is the whole conversion.
    #[must_use]
    pub fn to_godot(&self) -> Affine {
        // P maps (x, y, z) to (x, z, -y), so its rows pick and sign source
        // axes. Conjugating is P * basis * P transposed.
        const P: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]];
        let mut tmp = [[0.0f32; 3]; 3];
        for (r, row) in tmp.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| P[r][k] * self.basis[k][c]).sum();
            }
        }
        let mut basis = [[0.0f32; 3]; 3];
        for (r, row) in basis.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                // Multiplying by P transposed is indexing P by its column.
                *cell = (0..3).map(|k| tmp[r][k] * P[c][k]).sum();
            }
        }
        let o = self.origin;
        Affine {
            basis,
            origin: [
                o[0] * METRES_PER_UNIT,
                o[2] * METRES_PER_UNIT,
                -o[1] * METRES_PER_UNIT,
            ],
        }
    }
}

/// A skinned mesh's bone list resolved against a skeleton.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoneMap {
    /// One entry per mesh bone, in the mesh's own order: the index of that
    /// bone in the skeleton, or `None` when the skeleton does not name it.
    pub skeleton_index: Vec<Option<usize>>,
    /// Mesh bones matched only after lowercasing both names.
    pub matched_by_case: usize,
    /// Mesh bone names with no skeleton bone at all, in mesh order.
    pub unmatched: Vec<String>,
}

impl BoneMap {
    /// Match a mesh's bone list onto a skeleton by name.
    ///
    /// Exact names first, then a case insensitive pass, because the two file
    /// formats are authored by different tools and vanilla is not consistent
    /// about capitalisation. Anything still unmatched is reported rather than
    /// guessed at, and renders at its bind pose.
    #[must_use]
    pub fn build(mesh_bones: &[MeshBone], skeleton: &Skeleton) -> BoneMap {
        let mut exact = std::collections::HashMap::with_capacity(skeleton.bones.len());
        let mut folded = std::collections::HashMap::with_capacity(skeleton.bones.len());
        for (i, bone) in skeleton.bones.iter().enumerate() {
            exact.entry(bone.name.as_str()).or_insert(i);
            folded.entry(bone.name.to_lowercase()).or_insert(i);
        }
        let mut map = BoneMap {
            skeleton_index: Vec::with_capacity(mesh_bones.len()),
            matched_by_case: 0,
            unmatched: Vec::new(),
        };
        for bone in mesh_bones {
            if let Some(&i) = exact.get(bone.name.as_str()) {
                map.skeleton_index.push(Some(i));
            } else if let Some(&i) = folded.get(&bone.name.to_lowercase()) {
                map.matched_by_case += 1;
                map.skeleton_index.push(Some(i));
            } else {
                map.unmatched.push(bone.name.clone());
                map.skeleton_index.push(None);
            }
        }
        map
    }

    /// How many mesh bones found a skeleton bone, by either pass.
    #[must_use]
    pub fn matched(&self) -> usize {
        self.skeleton_index.iter().filter(|i| i.is_some()).count()
    }

    /// Mesh bones in the list.
    #[must_use]
    pub fn len(&self) -> usize {
        self.skeleton_index.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.skeleton_index.is_empty()
    }
}

/// Why a skinning pass could not run.
#[derive(Debug, Clone, PartialEq)]
pub enum SkinError {
    /// The map and the mesh's bone list are different lengths.
    MapLength { map: usize, bones: usize },
    /// A mapped skeleton index is past the end of the pose.
    PoseIndex { bone: usize, at: usize, pose: usize },
}

impl std::fmt::Display for SkinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkinError::MapLength { map, bones } => {
                write!(f, "bone map has {map} entries, the mesh has {bones} bones")
            }
            SkinError::PoseIndex { bone, at, pose } => write!(
                f,
                "mesh bone {bone} maps to skeleton bone {at}, past the {pose} bone pose"
            ),
        }
    }
}

impl std::error::Error for SkinError {}

/// One skinning transform per mesh bone, in Godot's axes and metres.
///
/// `model_pose` is the animated pose already lifted into model space by
/// [`soe_havok::pose::to_model_space`]. A mesh bone the skeleton does not name
/// gets the identity, which leaves the vertices weighted to it at their bind
/// position rather than collapsing them to the origin.
///
/// The transform is `pose * inverseBind`, with the bind read through
/// [`Affine::from_nif_skin`] because `NiSkinData` stores its rotations
/// transposed relative to a scene graph node. That transpose is the whole fix
/// for the flattened limbs, and leaving it out is invisible at the pelvis
/// because an identity rotation is its own transpose.
///
/// [`Skin::transform`] is **not** composed on the end, and `nifly` says why in
/// as many words: a bone's transform there is named `transformSkinToBone`, and
/// the shape's own is `transformGlobalToSkin`. Vertices arrive in skin space,
/// so bone world times skin to bone already carries them to model space, and
/// the global offset is not part of it. At the reference pose every bone then
/// produces the same constant, that offset's inverse, which is the property
/// `tests/real_skinning.rs` asserts. Composing the offset in as well cancels
/// it and buries the character 1.7 m under the floor, measured in
/// `tests/bind_pose.rs` rather than argued. An earlier attempt did compose it,
/// without the transpose, and reported a smaller error while rendering worse,
/// because the pelvis is the one bone a missing transpose does not spoil.
///
/// # Errors
///
/// [`SkinError`] when the map does not match the skin's bone list, or when a
/// mapped index is outside the pose.
pub fn skinning_transforms_into(
    map: &BoneMap,
    skin: &Skin,
    model_pose: &[QsTransform],
    out: &mut Vec<Affine>,
) -> Result<(), SkinError> {
    if map.len() != skin.bones.len() {
        return Err(SkinError::MapLength {
            map: map.len(),
            bones: skin.bones.len(),
        });
    }
    out.clear();
    out.reserve(skin.bones.len());
    for (bone, (slot, mesh_bone)) in map.skeleton_index.iter().zip(&skin.bones).enumerate() {
        let Some(at) = *slot else {
            out.push(Affine::IDENTITY);
            continue;
        };
        let Some(model) = model_pose.get(at) else {
            return Err(SkinError::PoseIndex {
                bone,
                at,
                pose: model_pose.len(),
            });
        };
        let full = Affine::from_qs(model).then(&Affine::from_nif_skin(&mesh_bone.inverse_bind));
        out.push(full.to_godot());
    }
    Ok(())
}

/// [`skinning_transforms_into`] with the vector allocated for you.
///
/// # Errors
///
/// As [`skinning_transforms_into`].
pub fn skinning_transforms(
    map: &BoneMap,
    skin: &Skin,
    model_pose: &[QsTransform],
) -> Result<Vec<Affine>, SkinError> {
    let mut out = Vec::new();
    skinning_transforms_into(map, skin, model_pose, &mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_havok::skeleton::Bone as SkeletonBone;
    use soe_nif::types::{Mat33, Vec3};

    fn skeleton(names: &[&str]) -> Skeleton {
        Skeleton {
            name: "test".into(),
            parent_indices: (0..names.len())
                .map(|i| if i == 0 { -1 } else { (i - 1) as i16 })
                .collect(),
            bones: names
                .iter()
                .map(|n| SkeletonBone {
                    name: (*n).to_string(),
                    lock_translation: false,
                })
                .collect(),
            reference_pose: vec![QsTransform::IDENTITY; names.len()],
            reference_floats: Vec::new(),
            float_slots: Vec::new(),
        }
    }

    fn mesh_bone(name: &str, inverse_bind: NifTransform) -> MeshBone {
        MeshBone {
            name: name.to_string(),
            node: soe_nif::types::BlockRef::NONE,
            inverse_bind,
            bound: soe_nif::types::Bound::default(),
        }
    }

    fn skin_of(bones: &[MeshBone], offset: NifTransform) -> Skin {
        Skin {
            skeleton_root: soe_nif::types::BlockRef::NONE,
            skeleton_root_name: "NPC Root [Root]".into(),
            transform: offset,
            bones: bones.to_vec(),
            body_parts: Vec::new(),
        }
    }

    fn translated_nif(x: f32, y: f32, z: f32) -> NifTransform {
        NifTransform {
            rotation: Mat33::IDENTITY,
            translation: Vec3 { x, y, z },
            scale: 1.0,
        }
    }

    #[test]
    fn an_identity_quaternion_gives_the_identity_basis() {
        let a = Affine::from_qs(&QsTransform::IDENTITY);
        assert_eq!(a.basis, Affine::IDENTITY.basis);
    }

    #[test]
    fn a_quarter_turn_about_z_sends_x_to_y() {
        let h = std::f32::consts::FRAC_PI_4;
        let q = QsTransform {
            rotation: [0.0, 0.0, h.sin(), h.cos()],
            ..QsTransform::IDENTITY
        };
        let v = Affine::from_qs(&q).apply([1.0, 0.0, 0.0]);
        assert!(v[0].abs() < 1e-6, "{v:?}");
        assert!((v[1] - 1.0).abs() < 1e-6, "{v:?}");
    }

    #[test]
    fn composition_matches_applying_one_transform_then_the_other() {
        let a = Affine::from_qs(&QsTransform {
            translation: [1.0, 2.0, 3.0, 0.0],
            ..QsTransform::IDENTITY
        });
        let b = Affine::from_nif(&translated_nif(10.0, 0.0, 0.0));
        let v = [0.5, 0.5, 0.5];
        let stepwise = a.apply(b.apply(v));
        let composed = a.then(&b).apply(v);
        for i in 0..3 {
            assert!((stepwise[i] - composed[i]).abs() < 1e-5, "{i}");
        }
    }

    #[test]
    fn the_godot_conversion_matches_the_position_conversion() {
        // A pure translation must land where units.rs would put the point.
        let a = Affine::from_nif(&translated_nif(70.0, 140.0, 210.0));
        let g = a.to_godot();
        let want = crate::units::SkyrimPos::new(70.0, 140.0, 210.0).to_godot();
        assert!((g.origin[0] - want.x).abs() < 1e-5, "{:?}", g.origin);
        assert!((g.origin[1] - want.y).abs() < 1e-5, "{:?}", g.origin);
        assert!((g.origin[2] - want.z).abs() < 1e-5, "{:?}", g.origin);
    }

    #[test]
    fn the_godot_conversion_keeps_a_rotation_a_rotation() {
        // Conjugation by a rotation preserves orthonormality, so the basis
        // determinant must stay +1 and a silent mirror is impossible.
        let h = std::f32::consts::FRAC_PI_3;
        let q = QsTransform {
            rotation: [h.sin(), 0.0, 0.0, h.cos()],
            ..QsTransform::IDENTITY
        };
        let b = Affine::from_qs(&q).to_godot().basis;
        let det = b[0][0] * (b[1][1] * b[2][2] - b[1][2] * b[2][1])
            - b[0][1] * (b[1][0] * b[2][2] - b[1][2] * b[2][0])
            + b[0][2] * (b[1][0] * b[2][1] - b[1][1] * b[2][0]);
        assert!((det - 1.0).abs() < 1e-5, "determinant {det}");
    }

    #[test]
    fn a_skyrim_rotation_about_up_becomes_a_godot_rotation_about_up() {
        // Skyrim's up is Z, Godot's is Y. A turn about Skyrim Z must come out
        // as a turn about Godot Y, which is what conjugation is for.
        let h = std::f32::consts::FRAC_PI_4;
        let q = QsTransform {
            rotation: [0.0, 0.0, h.sin(), h.cos()],
            ..QsTransform::IDENTITY
        };
        let g = Affine::from_qs(&q).to_godot();
        // Godot up must be fixed by the rotation.
        let up = g.apply([0.0, 1.0, 0.0]);
        assert!(
            up[0].abs() < 1e-5 && (up[1] - 1.0).abs() < 1e-5 && up[2].abs() < 1e-5,
            "{up:?}"
        );
    }

    #[test]
    fn bones_match_by_name_and_report_what_did_not() {
        let skel = skeleton(&["NPC Root [Root]", "NPC Spine [Spn0]"]);
        let bones = [
            mesh_bone("NPC Root [Root]", NifTransform::IDENTITY),
            mesh_bone("NPC Missing [Msng]", NifTransform::IDENTITY),
        ];
        let map = BoneMap::build(&bones, &skel);
        assert_eq!(map.skeleton_index, vec![Some(0), None]);
        assert_eq!(map.matched(), 1);
        assert_eq!(map.unmatched, vec!["NPC Missing [Msng]".to_string()]);
        assert_eq!(map.matched_by_case, 0);
    }

    #[test]
    fn a_name_that_differs_only_in_case_still_matches_and_is_counted() {
        let skel = skeleton(&["NPC Spine [Spn0]"]);
        let bones = [mesh_bone("npc spine [spn0]", NifTransform::IDENTITY)];
        let map = BoneMap::build(&bones, &skel);
        assert_eq!(map.skeleton_index, vec![Some(0)]);
        assert_eq!(map.matched_by_case, 1);
        assert!(map.unmatched.is_empty());
    }

    #[test]
    fn an_unmatched_bone_skins_at_its_bind_pose_rather_than_the_origin() {
        let skel = skeleton(&["A"]);
        let bones = [mesh_bone("B", translated_nif(99.0, 0.0, 0.0))];
        let map = BoneMap::build(&bones, &skel);
        let skin = skin_of(&bones, NifTransform::IDENTITY);
        let out = skinning_transforms(&map, &skin, &[QsTransform::IDENTITY]).unwrap();
        assert_eq!(out[0], Affine::IDENTITY);
    }

    #[test]
    fn the_bind_pose_cancels_when_the_bone_has_not_moved() {
        // Inverse bind of a bone one unit along X, and the bone sitting at
        // exactly that place: the skinning transform must be the identity, so
        // a character in its bind pose renders unchanged.
        let skel = skeleton(&["A"]);
        let bones = [mesh_bone("A", translated_nif(-70.0, 0.0, 0.0))];
        let map = BoneMap::build(&bones, &skel);
        let model = [QsTransform {
            translation: [70.0, 0.0, 0.0, 0.0],
            ..QsTransform::IDENTITY
        }];
        let skin = skin_of(&bones, NifTransform::IDENTITY);
        let out = skinning_transforms(&map, &skin, &model).unwrap();
        for i in 0..3 {
            assert!(out[0].origin[i].abs() < 1e-4, "{:?}", out[0].origin);
        }
    }

    #[test]
    fn a_moved_bone_moves_its_vertices_by_the_difference() {
        let skel = skeleton(&["A"]);
        // Bind at the origin, so the inverse bind is the identity.
        let bones = [mesh_bone("A", NifTransform::IDENTITY)];
        let map = BoneMap::build(&bones, &skel);
        // Bone lifted 70 Skyrim units along Skyrim Z, a shade over a metre.
        let model = [QsTransform {
            translation: [0.0, 0.0, 70.0, 0.0],
            ..QsTransform::IDENTITY
        }];
        let skin = skin_of(&bones, NifTransform::IDENTITY);
        let out = skinning_transforms(&map, &skin, &model).unwrap();
        // Skyrim Z up is Godot Y up, and 70 units is 70 * METRES_PER_UNIT.
        let want = 70.0 * METRES_PER_UNIT;
        assert!(
            (out[0].origin[1] - want).abs() < 1e-6,
            "{:?} wanted y {want}",
            out[0].origin
        );
    }

    /// A rotation about Z by `turns` of a quarter, as a `NIF` transform, so a
    /// test can tell a basis from its own transpose.
    fn rotated_nif(radians: f32) -> NifTransform {
        let (s, c) = radians.sin_cos();
        NifTransform {
            rotation: Mat33 {
                rows: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
            },
            translation: Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            scale: 1.0,
        }
    }

    #[test]
    fn a_skin_transform_is_read_transposed_where_a_node_transform_is_not() {
        // The whole bind pose defect in one assertion: the same bytes read two
        // ways give bases that are transposes of each other.
        let t = rotated_nif(0.7);
        let node = Affine::from_nif(&t);
        let skin = Affine::from_nif_skin(&t);
        for r in 0..3 {
            for c in 0..3 {
                assert!(
                    (node.basis[r][c] - skin.basis[c][r]).abs() < 1e-6,
                    "row {r} column {c}: {:?} against {:?}",
                    node.basis,
                    skin.basis
                );
            }
        }
        // A pure translation is its own transpose, which is why the pelvis hid
        // this for as long as it did.
        let flat = translated_nif(1.0, 2.0, 3.0);
        assert_eq!(
            Affine::from_nif(&flat).basis,
            Affine::from_nif_skin(&flat).basis
        );
    }

    #[test]
    fn a_rotated_bind_cancels_only_because_the_bind_is_transposed() {
        // A bone rotated about Z, with the mesh storing the inverse of that
        // rotation in NiSkinData's own convention. The skinning transform must
        // come back the identity, and would not if the basis were read as a
        // node transform.
        let angle = 0.7f32;
        let skel = skeleton(&["A"]);
        // What the file stores is the forward rotation, because writing the
        // inverse for `v * M` is writing the transpose of the inverse, and for
        // a rotation that is the rotation itself. Reading it back transposed
        // turns it into the inverse this multiplication needs.
        let bones = [mesh_bone("A", rotated_nif(angle))];
        let map = BoneMap::build(&bones, &skel);
        let (s, c) = (angle / 2.0).sin_cos();
        let model = [QsTransform {
            rotation: [0.0, 0.0, s, c],
            ..QsTransform::IDENTITY
        }];
        let skin = skin_of(&bones, NifTransform::IDENTITY);
        let out = skinning_transforms(&map, &skin, &model).unwrap();
        for r in 0..3 {
            for c2 in 0..3 {
                let want = if r == c2 { 1.0 } else { 0.0 };
                assert!(
                    (out[0].basis[r][c2] - want).abs() < 1e-5,
                    "basis {:?} is not the identity",
                    out[0].basis
                );
            }
        }
    }

    #[test]
    fn the_shapes_own_offset_is_left_for_the_vertices_to_carry() {
        // The shape's offset must not reach the skinning transform. A bone at
        // its bind position gives the identity here whatever the offset says,
        // because the vertices are already expressed relative to it.
        let skel = skeleton(&["A"]);
        let bones = [mesh_bone("A", NifTransform::IDENTITY)];
        let map = BoneMap::build(&bones, &skel);
        let offset = skin_of(&bones, translated_nif(0.0, 0.0, -120.0));
        let none = skin_of(&bones, NifTransform::IDENTITY);
        let with = skinning_transforms(&map, &offset, &[QsTransform::IDENTITY]).unwrap();
        let without = skinning_transforms(&map, &none, &[QsTransform::IDENTITY]).unwrap();
        assert_eq!(
            with[0], without[0],
            "the shape offset must not move a bone's skinning transform"
        );
    }

    #[test]
    fn a_map_of_the_wrong_length_is_refused() {
        let skel = skeleton(&["A"]);
        let bones = [mesh_bone("A", NifTransform::IDENTITY)];
        let map = BoneMap::build(&bones, &skel);
        let empty = skin_of(&[], NifTransform::IDENTITY);
        let err = skinning_transforms(&map, &empty, &[QsTransform::IDENTITY]).unwrap_err();
        assert_eq!(err, SkinError::MapLength { map: 1, bones: 0 });
    }

    #[test]
    fn a_pose_shorter_than_the_skeleton_is_refused_rather_than_wrapped() {
        let skel = skeleton(&["A", "B"]);
        let bones = [mesh_bone("B", NifTransform::IDENTITY)];
        let map = BoneMap::build(&bones, &skel);
        let skin = skin_of(&bones, NifTransform::IDENTITY);
        let err = skinning_transforms(&map, &skin, &[QsTransform::IDENTITY]).unwrap_err();
        assert_eq!(
            err,
            SkinError::PoseIndex {
                bone: 0,
                at: 1,
                pose: 1
            }
        );
    }
}
