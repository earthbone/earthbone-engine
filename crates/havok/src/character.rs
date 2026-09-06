// SPDX-License-Identifier: GPL-3.0-or-later

//! `hkbProjectData` (the `*project.hkx` files) and `hkbCharacterData` (the
//! `characters/*.hkx` files). Between them they name the rig, the ragdoll,
//! the behavior file and the animation list a character uses.

use crate::behavior::{VariableInfo, VariableValueSet};
use crate::packfile::{root_variant_of, Obj};
use crate::{Packfile, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectData {
    pub world_up_ws: [f32; 4],
    pub default_event_mode: i8,
    pub animation_filenames: Vec<String>,
    pub behavior_filenames: Vec<String>,
    pub character_filenames: Vec<String>,
    pub event_names: Vec<String>,
    pub animation_path: String,
    pub behavior_path: String,
    pub character_path: String,
    pub full_path_to_source: String,
}

impl ProjectData {
    /// hkbProjectData: worldUpWS 16, stringData 32, defaultEventMode 40.
    /// hkbProjectStringData: animationFilenames 16, behaviorFilenames 32,
    /// characterFilenames 48, eventNames 64, animationPath 80,
    /// behaviorPath 88, characterPath 96, fullPathToSource 104.
    pub fn from_obj(o: Obj<'_>) -> Result<ProjectData> {
        o.expect_class("hkbProjectData")?;
        let s = o.ptr_required(32, "hkbProjectData.stringData")?;
        s.expect_class("hkbProjectStringData")?;
        Ok(ProjectData {
            world_up_ws: o.vec4(16)?,
            default_event_mode: o.i8(40)?,
            animation_filenames: s.array_strings(16)?,
            behavior_filenames: s.array_strings(32)?,
            character_filenames: s.array_strings(48)?,
            event_names: s.array_strings(64)?,
            animation_path: s.string_or_empty(80)?,
            behavior_path: s.string_or_empty(88)?,
            character_path: s.string_or_empty(96)?,
            full_path_to_source: s.string_or_empty(104)?,
        })
    }

    pub fn from_packfile(pf: &Packfile) -> Result<ProjectData> {
        pf.require_64bit()?;
        Self::from_obj(root_variant_of(pf, "hkbProjectData")?)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterData {
    pub model_up_ms: [f32; 4],
    pub model_forward_ms: [f32; 4],
    pub model_right_ms: [f32; 4],
    pub character_property_infos: Vec<VariableInfo>,
    pub num_bones_per_lod: Vec<i32>,
    pub character_property_values: Option<VariableValueSet>,
    pub scale: f32,
    pub deformable_skin_names: Vec<String>,
    pub rigid_skin_names: Vec<String>,
    pub animation_names: Vec<String>,
    pub animation_filenames: Vec<String>,
    pub character_property_names: Vec<String>,
    pub retargeting_skeleton_mapper_filenames: Vec<String>,
    pub lod_names: Vec<String>,
    pub mirrored_sync_point_substrings_a: Vec<String>,
    pub mirrored_sync_point_substrings_b: Vec<String>,
    pub name: String,
    pub rig_name: String,
    pub ragdoll_name: String,
    pub behavior_filename: String,
    pub has_foot_ik_driver_info: bool,
    pub has_hand_ik_driver_info: bool,
    pub has_mirrored_skeleton_info: bool,
}

impl CharacterData {
    /// hkbCharacterData: characterControllerInfo 16, modelUpMS 48,
    /// modelForwardMS 64, modelRightMS 80, characterPropertyInfos 96,
    /// numBonesPerLod 112, characterPropertyValues 128, footIkDriverInfo
    /// 136, handIkDriverInfo 144, stringData 152, mirroredSkeletonInfo 160,
    /// scale 168. hkbCharacterStringData: deformableSkinNames 16,
    /// rigidSkinNames 32, animationNames 48, animationFilenames 64,
    /// characterPropertyNames 80, retargetingSkeletonMapperFilenames 96,
    /// lodNames 112, mirroredSyncPointSubstringsA 128, ...B 144, name 160,
    /// rigName 168, ragdollName 176, behaviorFilename 184.
    pub fn from_obj(o: Obj<'_>) -> Result<CharacterData> {
        o.expect_class("hkbCharacterData")?;
        let s = o.ptr_required(152, "hkbCharacterData.stringData")?;
        s.expect_class("hkbCharacterStringData")?;
        Ok(CharacterData {
            model_up_ms: o.vec4(48)?,
            model_forward_ms: o.vec4(64)?,
            model_right_ms: o.vec4(80)?,
            character_property_infos: VariableInfo::array(o, 96)?,
            num_bones_per_lod: o.array_i32(112)?,
            character_property_values: o.ptr(128)?.map(VariableValueSet::from_obj).transpose()?,
            scale: o.f32(168)?,
            deformable_skin_names: s.array_strings(16)?,
            rigid_skin_names: s.array_strings(32)?,
            animation_names: s.array_strings(48)?,
            animation_filenames: s.array_strings(64)?,
            character_property_names: s.array_strings(80)?,
            retargeting_skeleton_mapper_filenames: s.array_strings(96)?,
            lod_names: s.array_strings(112)?,
            mirrored_sync_point_substrings_a: s.array_strings(128)?,
            mirrored_sync_point_substrings_b: s.array_strings(144)?,
            name: s.string_or_empty(160)?,
            rig_name: s.string_or_empty(168)?,
            ragdoll_name: s.string_or_empty(176)?,
            behavior_filename: s.string_or_empty(184)?,
            has_foot_ik_driver_info: o.ptr(136)?.is_some(),
            has_hand_ik_driver_info: o.ptr(144)?.is_some(),
            has_mirrored_skeleton_info: o.ptr(160)?.is_some(),
        })
    }

    pub fn from_packfile(pf: &Packfile) -> Result<CharacterData> {
        pf.require_64bit()?;
        Self::from_obj(root_variant_of(pf, "hkbCharacterData")?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::PackfileBuilder;

    #[test]
    fn character_data_reads_its_string_data() {
        let mut b = PackfileBuilder::new();
        let root = b.object("hkRootLevelContainer", 16);
        let cd = b.object("hkbCharacterData", 176);
        let sd = b.object("hkbCharacterStringData", 192);
        let variant = b.array_structs(root, 0, 24, 1);
        b.string(variant, 0, "hkbCharacterData");
        b.string(variant, 8, "hkbCharacterData");
        b.ptr(variant, 16, cd);
        b.ptr(cd, 152, sd);
        b.set_f32(cd, 168, 1.0);
        b.set_vec4(cd, 48, [0.0, 0.0, 1.0, 0.0]);
        b.array_i32(cd, 112, &[99]);
        b.array_strings(sd, 48, &["mt_idle", "mt_walkforward"]);
        b.string(sd, 160, "DefaultMale");
        b.string(sd, 168, r"Character Assets\skeleton.hkx");
        b.string(sd, 184, r"Behaviors\0_Master.hkx");
        let bytes = b.finish();
        let pf = Packfile::parse(&bytes).unwrap();
        let c = CharacterData::from_packfile(&pf).unwrap();
        assert_eq!(c.name, "DefaultMale");
        assert_eq!(c.animation_names, vec!["mt_idle", "mt_walkforward"]);
        assert_eq!(c.num_bones_per_lod, vec![99]);
        assert_eq!(c.model_up_ms[2], 1.0);
        assert!(c.behavior_filename.ends_with("0_Master.hkx"));
        assert!(c.character_property_values.is_none());
    }
}
