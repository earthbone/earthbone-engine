// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Authored motion fixtures use serde-hkx's reference-frame layout. The
//! installed corpus checks all samples against the original packed vectors.

use soe_hkx::animation::SplineCompressedAnimation;
use soe_hkx::motion::ReferenceFrame;
use soe_hkx::writer::PackfileBuilder;
use soe_hkx::{Error, Packfile, QsTransform};

fn fixture(samples: &[[f32; 4]], duration: f32, up: [f32; 4]) -> Vec<u8> {
    let mut b = PackfileBuilder::new();
    let animation = b.object("hkaSplineCompressedAnimation", 176);
    b.set_f32(animation, 20, duration);
    let motion = b.object("hkaDefaultAnimatedReferenceFrame", 80);
    b.ptr(animation, 32, motion);
    for (i, value) in up.iter().enumerate() {
        b.set_f32(motion, 16 + i as u32 * 4, *value);
    }
    b.set_f32(motion, 36, 1.0);
    b.set_f32(motion, 48, duration);
    let data = b.array_structs(motion, 56, 16, samples.len());
    for (i, sample) in samples.iter().enumerate() {
        for (axis, value) in sample.iter().enumerate() {
            b.set_f32(data, (i * 16 + axis * 4) as u32, *value);
        }
    }
    b.finish()
}

fn read(bytes: &[u8]) -> soe_hkx::Result<ReferenceFrame> {
    let pf = Packfile::parse(bytes)?;
    ReferenceFrame::from_obj(pf.objects_of("hkaDefaultAnimatedReferenceFrame")[0])
}

fn assert_near(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-5, "{a} != {b}");
}

#[test]
fn anm_05_authored_translation_and_rotation_sample_over_clip_lifetime() {
    let samples = [
        [0.0, 0.0, 0.0, 0.0],
        [4.0, 8.0, 2.0, 1.0],
        [8.0, 0.0, 4.0, 2.0],
    ];
    let bytes = fixture(&samples, 2.0, [0.0, 0.0, 1.0, 0.0]);
    let pf = Packfile::parse(&bytes).unwrap();
    let animation = SplineCompressedAnimation::from_obj(pf.root().unwrap()).unwrap();
    let motion = animation.extracted_motion.unwrap();
    assert_eq!(motion.samples(), samples);
    assert_eq!(motion.forward(), [0.0, 1.0, 0.0, 0.0]);
    for (time, expected) in [
        (-1.0, [0.0, 0.0, 0.0, 0.0]),
        (0.5, [2.0, 4.0, 1.0, 0.5]),
        (1.0, [4.0, 8.0, 2.0, 1.0]),
        (1.5, [6.0, 4.0, 3.0, 1.5]),
        (3.0, [8.0, 0.0, 4.0, 2.0]),
    ] {
        let pose = motion.sample(time).unwrap();
        assert_eq!(&pose.translation[..3], &expected[..3]);
        assert_near(pose.rotation[2], (expected[3] * 0.5_f32).sin());
        assert_near(pose.rotation[3], (expected[3] * 0.5_f32).cos());
    }
}

#[test]
fn heading_wrap_uses_shortest_arc_and_authored_up_axis() {
    let samples = [
        [0.0, 0.0, 0.0, 170_f32.to_radians()],
        [0.0, 0.0, 0.0, -170_f32.to_radians()],
    ];
    let motion = read(&fixture(&samples, 1.0, [1.0, 0.0, 0.0, 0.0])).unwrap();
    let mid = motion.sample(0.5).unwrap();
    assert_near(mid.rotation[0].abs(), 1.0);
    assert_near(mid.rotation[1], 0.0);
    assert_near(mid.rotation[2], 0.0);
    assert_near(mid.rotation[3], 0.0);
}

#[test]
fn empty_and_single_sample_frames_do_not_divide_by_zero() {
    let up = [0.0, 0.0, 1.0, 0.0];
    let empty = read(&fixture(&[], 0.0, up)).unwrap();
    assert_eq!(empty.sample(1.0).unwrap(), QsTransform::IDENTITY);
    let single = read(&fixture(&[[1.0, 2.0, 3.0, 0.0]], 0.0, up)).unwrap();
    assert_eq!(single.sample(-2.0), single.sample(20.0));
}

#[test]
fn invalid_motion_values_return_typed_errors() {
    let up = [0.0, 0.0, 1.0, 0.0];
    for bytes in [
        fixture(&[[0.0; 4]; 2], 0.0, up),
        fixture(&[], -1.0, up),
        fixture(&[], f32::NAN, up),
        fixture(&[[f32::INFINITY; 4]], 1.0, up),
        fixture(&[], 1.0, [0.0; 4]),
    ] {
        assert!(matches!(read(&bytes), Err(Error::BadMotion(_))));
    }
    let motion = read(&fixture(&[], 0.0, up)).unwrap();
    assert!(matches!(motion.sample(f32::NAN), Err(Error::BadMotion(_))));
}

#[test]
fn absent_motion_stays_absent_and_wrong_class_is_rejected() {
    let mut b = PackfileBuilder::new();
    b.object("hkaSplineCompressedAnimation", 176);
    let pf = Packfile::parse(&b.finish()).unwrap();
    assert!(SplineCompressedAnimation::from_obj(pf.root().unwrap())
        .unwrap()
        .extracted_motion
        .is_none());
    assert!(matches!(
        ReferenceFrame::from_obj(pf.root().unwrap()),
        Err(Error::WrongClass { .. })
    ));
}

#[test]
fn installed_extracted_motion_matches_every_packed_sample() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let archive =
        soe_bsa::Archive::open(&std::path::Path::new(&data).join("Skyrim - Animations.bsa"))
            .unwrap();
    let mut paths: Vec<_> = archive
        .files()
        .iter()
        .filter(|p| p.to_ascii_lowercase().ends_with(".hkx"))
        .collect();
    paths.sort();
    let mut frames = 0;
    let mut samples = 0;
    let mut files = 0;
    let mut animations = 0;
    let mut pointers = 0;
    for path in paths {
        let bytes = archive.read(path).unwrap();
        let pf = Packfile::parse(&bytes).unwrap();
        files += 1;
        for object in pf.objects_of("hkaSplineCompressedAnimation") {
            animations += 1;
            if let Some(reference) = object.ptr(32).unwrap() {
                let animation = SplineCompressedAnimation::from_obj(object).unwrap();
                assert_eq!(
                    animation.extracted_motion,
                    Some(Box::new(ReferenceFrame::from_obj(reference).unwrap())),
                    "{path}"
                );
                pointers += 1;
            }
        }
        for object in pf.objects_of("hkaDefaultAnimatedReferenceFrame") {
            let motion = ReferenceFrame::from_obj(object).unwrap_or_else(|e| panic!("{path}: {e}"));
            let raw = object.array_vec4(56).unwrap();
            assert_eq!(motion.samples(), raw, "{path}");
            for (i, expected) in raw.iter().enumerate() {
                let time = if raw.len() > 1 {
                    i as f32 / (raw.len() - 1) as f32 * motion.duration()
                } else {
                    0.0
                };
                let actual = motion.sample(time).unwrap();
                for (axis, value) in expected.iter().enumerate().take(3) {
                    let tolerance = 2e-4 * value.abs().max(1.0);
                    assert!(
                        (actual.translation[axis] - value).abs() < tolerance,
                        "{path} sample {i} axis {axis}"
                    );
                }
                let (sin, cos) = (expected[3] * 0.5).sin_cos();
                let up = motion.up();
                let expected_rotation = [up[0] * sin, up[1] * sin, up[2] * sin, cos];
                let dot: f32 = actual
                    .rotation
                    .iter()
                    .zip(expected_rotation)
                    .map(|(a, b)| a * b)
                    .sum();
                assert!((dot.abs() - 1.0).abs() < 1e-4, "{path} sample {i} rotation");
                samples += 1;
            }
            frames += 1;
        }
    }
    println!("extracted motion: {files} packfiles, {animations} animations, {pointers} pointers, {frames} frames, {samples} samples");
    // CONTEXT: Corpus totals were recorded before ANM-05 in docs/hkx.md.
    assert_eq!(files, 7699, "the full installed packfile corpus must run");
    assert_eq!(animations, 6126, "every shipped spline animation must run");
    assert!(pointers > 0, "exercise the optional extracted-motion field");
    assert_eq!(
        pointers, frames,
        "all authored frames must be reached from clips"
    );
}
