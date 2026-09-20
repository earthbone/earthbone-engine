// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::sound::{prepare, SoundSettings};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOUND: u32 = 0x800;
const MARKER: u32 = 0x801;
const CATEGORY: u32 = 0x900;
const PARENT: u32 = 0x901;
const OUTPUT: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn descriptor() -> Record {
    Record::new(T(b"SNDR"), SOUND)
        .sub(T(b"CNAM"), 0x1EEF_540A_u32.to_le_bytes())
        .sub(T(b"GNAM"), CATEGORY.to_le_bytes())
        .sub(T(b"ONAM"), OUTPUT.to_le_bytes())
        .zsub(T(b"ANAM"), "Data\\Sound\\FX\\First.wav")
        .zsub(T(b"ANAM"), "Sound/FX/Second.wav")
        .sub(T(b"BNAM"), [246, 5, 64, 6, 0xE8, 3])
        .sub(T(b"LNAM"), [1, 0, 0, 0])
}

fn marker_with_id(form_id: u32, descriptor: u32) -> Record {
    Record::new(T(b"SOUN"), form_id)
        .zsub(T(b"EDID"), "TestSoundMarker")
        .sub(T(b"SDSC"), descriptor.to_le_bytes())
}

fn marker_with(descriptor: u32) -> Record {
    marker_with_id(MARKER, descriptor)
}

fn marker() -> Record {
    marker_with(SOUND)
}

fn category(object: u32, flags: u32) -> Record {
    Record::new(T(b"SNCT"), object)
        .sub(T(b"FNAM"), flags.to_le_bytes())
        .sub(T(b"VNAM"), u16::MAX.to_le_bytes())
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(descriptor());
    plugin.push_top(marker());
    plugin.push_top(
        category(CATEGORY, 2)
            .sub(T(b"UNAM"), 32768_u16.to_le_bytes())
            .sub(T(b"PNAM"), PARENT.to_le_bytes()),
    );
    plugin.push_top(category(PARENT, 1));
    let mut attenuation = vec![0; 4];
    attenuation.extend(150_f32.to_le_bytes());
    attenuation.extend(1800_f32.to_le_bytes());
    attenuation.extend([100, 50, 20, 5, 0, 0, 0, 0]);
    plugin.push_top(
        Record::new(T(b"SOPM"), OUTPUT)
            .sub(T(b"NAM1"), [1, 0, 0, 30])
            .sub(T(b"MNAM"), 0_u32.to_le_bytes())
            .sub(T(b"ANAM"), attenuation),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

#[test]
fn named_sound_marker_resolves_to_its_descriptor_path_and_output() {
    let playback = prepare(&order(), id(MARKER), [0.0; 3], &SoundSettings::default()).unwrap();
    assert_eq!(playback.descriptor, id(SOUND));
    assert_eq!(playback.data_path, "sound/fx/first.wav");
    assert_eq!(playback.output.kind, 0);
}

#[test]
fn marker_override_resolves_sdsc_in_its_reordered_master_namespace() {
    let mut order = order();
    let mut other = Plugin::new(FileHeader::default());
    other.push_top(Record::new(T(b"SNDR"), SOUND));
    order.add("Other.esm", other).unwrap();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Other.esm".into(), "Skyrim.esm".into()],
        ..Default::default()
    });
    plugin.push_top(marker_with_id(0x0100_0801, 0x0100_0800));
    order.add("Marker.esp", plugin).unwrap();

    assert_eq!(order.winner_ref(id(MARKER)).unwrap().file, 2);
    let playback = prepare(&order, id(MARKER), [0.0; 3], &SoundSettings::default()).unwrap();
    assert_eq!(playback.descriptor, id(SOUND));
    assert_eq!(playback.data_path, "sound/fx/first.wav");
}

#[test]
fn malformed_null_deleted_and_wrong_target_markers_are_rejected() {
    let mut missing = order();
    patch(
        &mut missing,
        Record::new(T(b"SOUN"), MARKER).zsub(T(b"EDID"), "MissingDescriptor"),
    );
    assert!(
        prepare(&missing, id(MARKER), [0.0; 3], &SoundSettings::default())
            .unwrap_err()
            .to_string()
            .contains("has no descriptor")
    );

    let mut null = order();
    patch(&mut null, marker_with(0));
    assert!(
        prepare(&null, id(MARKER), [0.0; 3], &SoundSettings::default())
            .unwrap_err()
            .to_string()
            .contains("has no descriptor")
    );

    let mut deleted = order();
    patch(
        &mut deleted,
        marker().with_flags(soe_esm::plugin::flags::DELETED),
    );
    assert!(
        prepare(&deleted, id(MARKER), [0.0; 3], &SoundSettings::default())
            .unwrap_err()
            .to_string()
            .contains("expected live SNDR or SOUN")
    );

    let mut wrong = order();
    patch(&mut wrong, marker_with(CATEGORY));
    assert!(
        prepare(&wrong, id(MARKER), [0.0; 3], &SoundSettings::default())
            .unwrap_err()
            .to_string()
            .contains("expected live SNDR")
    );
}

fn patch(order: &mut LoadOrder, record: Record) {
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    plugin.push_top(record);
    order.add("Patch.esp", plugin).unwrap();
}

#[test]
fn variation_normalizes_paths_and_applies_signed_pitch_and_decibels() {
    let order = order();
    let settings = SoundSettings::default();
    let quiet = prepare(&order, id(SOUND), [0.0, 0.0, 1.0], &settings).unwrap();
    let loud = prepare(&order, id(SOUND), [1.0, 1.0, 0.0], &settings).unwrap();
    assert_eq!(quiet.data_path, "sound/fx/first.wav");
    assert_eq!(loud.data_path, "sound/fx/second.wav");
    assert!((quiet.pitch - 0.85).abs() < 0.00001);
    assert!((loud.pitch - 0.95).abs() < 0.00001);
    let menu = 32768.0 / 65535.0;
    assert!((quiet.gain - menu * 10_f32.powf(-16.0 / 20.0)).abs() < 0.00001);
    assert!((loud.gain - menu * 10_f32.powf(-10.0 / 20.0)).abs() < 0.00001);
    assert_eq!(loud.priority, 64);
    assert_eq!(loud.output.kind, 0);
    assert!(loud.output.attenuates);
    let curve = loud.output.attenuation.unwrap();
    assert_eq!((curve.min_distance, curve.max_distance), (150.0, 1800.0));
    assert_eq!(curve.curve, [100, 50, 20, 5, 0]);
}

#[test]
fn category_sliders_and_parent_underwater_mute_apply() {
    let order = order();
    let mut settings = SoundSettings::default();
    settings.category_volumes.insert(id(CATEGORY), 0.25);
    let sound = prepare(&order, id(SOUND), [0.5; 3], &settings).unwrap();
    assert!((sound.gain - 0.25 * 10_f32.powf(-13.0 / 20.0)).abs() < 0.00001);
    settings.submerged = true;
    assert_eq!(
        prepare(&order, id(SOUND), [0.5; 3], &settings)
            .unwrap()
            .gain,
        0.0
    );
}

#[test]
fn conditional_descriptors_are_not_played_unconditionally() {
    let mut order = order();
    patch(&mut order, descriptor().sub(T(b"CTDA"), [0; 32]));
    let error = prepare(&order, id(SOUND), [0.5; 3], &SoundSettings::default()).unwrap_err();
    assert!(error.to_string().contains("conditions"));
}

#[test]
fn invalid_draws_and_category_cycles_return_errors() {
    let mut order = order();
    assert!(prepare(
        &order,
        id(SOUND),
        [f32::NAN, 0.0, 0.0],
        &SoundSettings::default()
    )
    .is_err());
    patch(
        &mut order,
        category(PARENT, 0).sub(T(b"PNAM"), CATEGORY.to_le_bytes()),
    );
    let error = prepare(&order, id(SOUND), [0.5; 3], &SoundSettings::default()).unwrap_err();
    assert!(error.to_string().contains("cycle"));
}

#[test]
fn override_resolves_output_in_its_own_file_namespace() {
    let mut order = order();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let mut sound = descriptor();
    sound
        .subrecords
        .iter_mut()
        .find(|s| s.tag == T(b"ONAM"))
        .unwrap()
        .data = 0x0100_0800_u32.to_le_bytes().to_vec();
    plugin.push_top(sound);
    plugin.push_top(
        Record::new(T(b"SOPM"), 0x0100_0800)
            .sub(T(b"NAM1"), [2, 0, 0, 80])
            .sub(T(b"MNAM"), 1_u32.to_le_bytes())
            .sub(T(b"ONAM"), [100; 24]),
    );
    order.add("Output.esp", plugin).unwrap();
    let output = prepare(&order, id(SOUND), [0.5; 3], &SoundSettings::default())
        .unwrap()
        .output;
    assert_eq!(output.kind, 1);
    assert!(!output.attenuates);
    assert_eq!(output.speaker_matrix, Some([100; 24]));
}
