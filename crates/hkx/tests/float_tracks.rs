// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::interp::Clip;
use soe_hkx::spline::decode_block;
use soe_hkx::QsTransform;

fn animation(data: Vec<u8>, float_tracks: usize, float_start: u32) -> SplineCompressedAnimation {
    SplineCompressedAnimation {
        duration: 2.0,
        num_transform_tracks: 1,
        num_float_tracks: float_tracks,
        annotation_tracks: vec![],
        num_frames: 3,
        num_blocks: 1,
        max_frames_per_block: 3,
        mask_and_quantization_size: (4 + float_tracks).next_multiple_of(4) as i32,
        block_duration: 2.0,
        block_inverse_duration: 0.5,
        frame_duration: 1.0,
        block_offsets: vec![0],
        float_block_offsets: vec![float_start],
        transform_offsets: vec![],
        float_offsets: vec![],
        data,
        endian: 0,
        extracted_motion: None,
    }
}

fn block(bits16: bool) -> (Vec<u8>, u32) {
    // One static translation, then identity, static and linear float tracks.
    let mut data = vec![0, 1, 0, 0, 0, 1, if bits16 { 0x12 } else { 0x10 }, 0];
    data.extend_from_slice(&9.0f32.to_le_bytes());
    let start = data.len() as u32;
    data.extend_from_slice(&7.0f32.to_le_bytes());
    data.extend_from_slice(&[1, 0, 1, 0, 0, 2, 2]);
    data.resize(data.len().next_multiple_of(4), 0);
    data.extend_from_slice(&(-2.0f32).to_le_bytes());
    data.extend_from_slice(&6.0f32.to_le_bytes());
    if bits16 {
        data.extend_from_slice(&[0, 0, 255, 255]);
    } else {
        data.extend_from_slice(&[0, 255]);
    }
    data.resize(data.len().next_multiple_of(4), 0);
    (data, start)
}

#[test]
fn anm_12_named_float_slots_sample_authored_curves_without_remapping_bones() {
    for bits16 in [false, true] {
        let (data, start) = block(bits16);
        let clip = Clip::from_parts_with_float_binding(
            "slots",
            animation(data, 3, start),
            &[2],
            &[1, -1, 0],
        )
        .unwrap();
        let names = ["Weapon", "Shield", "Unbound"];
        let mut values = vec![20.0, 30.0, 40.0];
        clip.sample_floats_into(1.0, &mut values);
        let slots: std::collections::BTreeMap<_, _> = names.into_iter().zip(values).collect();
        assert_eq!(slots["Weapon"], 2.0);
        assert_eq!(slots["Shield"], 0.0);
        assert_eq!(slots["Unbound"], 40.0);
        assert_eq!(clip.track_to_bone(), &[2]);
        assert_eq!(clip.track_to_float_slot(), &[1, usize::MAX, 0]);
        let mut pose = vec![QsTransform::IDENTITY; 3];
        clip.sample_into(1.0, &mut pose);
        assert_eq!(pose[0], QsTransform::IDENTITY);
        assert_eq!(pose[1], QsTransform::IDENTITY);
        assert_eq!(pose[2].translation[0], 9.0);
    }
}

#[test]
fn identity_static_and_quantized_curves_sample_at_endpoints_and_midpoints() {
    for bits16 in [false, true] {
        let (data, start) = block(bits16);
        let anim = animation(data, 3, start);
        let blocks = anim.decode().unwrap();
        for (time, expected) in [
            (-1.0, -2.0),
            (0.0, -2.0),
            (0.5, 0.0),
            (1.0, 2.0),
            (2.0, 6.0),
            (4.0, 6.0),
        ] {
            assert_eq!(anim.sample_float(&blocks, 0, time), Some(0.0));
            assert_eq!(anim.sample_float(&blocks, 1, time), Some(7.0));
            assert_eq!(anim.sample_float(&blocks, 2, time), Some(expected));
        }
        assert_eq!(anim.sample_float(&blocks, 3, 0.0), None);
    }
}

#[test]
fn float_offsets_are_relative_to_each_block_and_can_skip_padding() {
    let (mut data, start) = block(false);
    let mut second = data.clone();
    second.splice(start as usize..start as usize, [0; 8]);
    let next = data.len() as u32;
    data.extend(second);
    let mut anim = animation(data, 3, start);
    anim.num_frames = 5;
    anim.duration = 4.0;
    anim.num_blocks = 2;
    anim.block_offsets.push(next);
    anim.float_block_offsets.push(start + 8);
    let blocks = anim.decode().unwrap();
    assert_eq!(anim.sample_float(&blocks, 2, 2.0), Some(-2.0));
    assert_eq!(anim.sample_float(&blocks, 2, 2.5), Some(0.0));
    assert_eq!(anim.sample_float(&blocks, 2, 3.0), Some(2.0));
    assert_eq!(anim.sample_float(&blocks, 2, 4.0), Some(6.0));
}

#[test]
fn malformed_float_payloads_and_cross_block_reads_are_rejected() {
    let (data, start) = block(false);
    for mask in [0x04, 0x08, 0x11, 0x20, 0xff] {
        let mut bytes = data.clone();
        bytes[6] = mask;
        assert!(animation(bytes, 3, start).decode().is_err(), "mask {mask}");
    }
    for offset in [0, 4, u32::MAX] {
        assert!(animation(data.clone(), 3, offset).decode().is_err());
    }
    for (at, value) in [(18, 3), (19, 2), (20, 3)] {
        let mut bytes = data.clone();
        bytes[at] = value;
        assert!(animation(bytes, 3, start).decode().is_err(), "byte {at}");
    }
    let mut bytes = data.clone();
    bytes[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(animation(bytes, 3, start).decode().is_err());
    for end in 0..34 {
        assert!(
            animation(data[..end].to_vec(), 3, start).decode().is_err(),
            "length {end}"
        );
    }
    let mut anim = animation(data, 3, start);
    anim.num_blocks = 2;
    anim.block_offsets.push(32);
    anim.float_block_offsets.push(4);
    assert!(anim.decode().is_err());
}

#[test]
fn standalone_block_decoding_reads_floats_after_transforms() {
    let (bytes, _) = block(false);
    let block = decode_block(&bytes, 0, 1, 3).unwrap();
    assert_eq!(block.float_tracks[2].sample(1.0), 2.0);
    assert_eq!(block.tracks[0].sample(1.0).translation[0], 9.0);
}

#[test]
fn installed_float_tracks_decode_and_sample_without_changing_transform_counts() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let archive =
        soe_bsa::Archive::open(&std::path::PathBuf::from(data).join("Skyrim - Animations.bsa"))
            .unwrap();
    let mut files = 0;
    let mut animations = 0;
    let mut float_tracks = 0;
    let mut failures = Vec::new();
    for name in archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".hkx"))
    {
        files += 1;
        let bytes = archive.read(name).unwrap();
        let packfile = soe_hkx::Packfile::parse(&bytes).unwrap();
        for object in packfile.objects_of("hkaSplineCompressedAnimation") {
            animations += 1;
            let animation = SplineCompressedAnimation::from_obj(object).unwrap();
            let blocks = match animation.decode() {
                Ok(blocks) => blocks,
                Err(error) => {
                    failures.push(format!("{name}: {error}"));
                    continue;
                }
            };
            float_tracks += animation.num_float_tracks;
            for block in blocks {
                assert_eq!(block.tracks.len(), animation.num_transform_tracks);
                assert_eq!(block.float_tracks.len(), animation.num_float_tracks);
                for track in block.float_tracks {
                    for frame in [0.0, 0.5, (animation.max_frames_per_block - 1) as f32] {
                        assert!(track.sample(frame).is_finite(), "{name}");
                    }
                }
            }
        }
    }
    println!(
        "{files} packfiles, {animations} animations, {float_tracks} float tracks, {} failures",
        failures.len()
    );
    assert!(files >= 7699);
    assert!(animations >= 6126);
    assert!(float_tracks > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn container_binding_carries_float_slots_separately_from_bones() {
    use soe_hkx::animation::Animation;
    use soe_hkx::skeleton::{AnimationBinding, AnimationContainer, BlendHint};
    let (data, start) = block(true);
    let mut container = AnimationContainer {
        skeletons: vec![],
        animations: vec![Animation::SplineCompressed(animation(data, 3, start))],
        bindings: vec![AnimationBinding {
            original_skeleton_name: "skeleton".into(),
            animation: Some(0),
            animation_offset: None,
            transform_track_to_bone_indices: vec![2],
            float_track_to_float_slot_indices: vec![1, 2, 0],
            blend_hint: BlendHint::Normal,
        }],
    };
    container.animations.insert(
        0,
        Animation::Unsupported {
            class: "hkaInterleavedUncompressedAnimation".into(),
            animation_type: 1,
            duration: 1.0,
        },
    );
    container.bindings[0].animation = Some(1);
    let mut unrelated = container.bindings[0].clone();
    unrelated.animation = Some(0);
    unrelated.transform_track_to_bone_indices = vec![0];
    unrelated.float_track_to_float_slot_indices = vec![2, 1, 0];
    container.bindings.insert(0, unrelated);
    let clip = Clip::from_container("bound", &container).unwrap();
    let mut values = [0.0; 3];
    clip.sample_floats_into(1.0, &mut values);
    assert_eq!(values, [2.0, 0.0, 7.0]);
    assert_eq!(clip.track_to_bone(), &[2]);
}

#[test]
fn malformed_timing_is_rejected_and_public_sampling_cannot_panic() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
        let (data, start) = block(false);
        let mut anim = animation(data, 3, start);
        anim.frame_duration = invalid;
        assert!(anim.decode().is_err());
        assert_eq!(anim.locate(1.0), (0, 0.0));
    }
    for invalid in [f32::NAN, f32::INFINITY, -1.0] {
        let (data, start) = block(false);
        let mut anim = animation(data, 3, start);
        anim.block_duration = invalid;
        assert!(anim.decode().is_err());
        assert_eq!(anim.locate(1.0), (0, 0.0));
    }
}
