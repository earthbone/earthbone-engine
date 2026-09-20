// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::sound::{prepare, prepare_for, SoundSettings};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SOUND: u32 = 0x800;
const CATEGORY: u32 = 0x900;
const PARENT: u32 = 0x901;
const OUTPUT: u32 = 0xA00;
const SOURCE: u32 = 0xB00;
const BASE: u32 = 0xB01;
const OTHER_BASE: u32 = 0xB02;

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

fn category(object: u32, flags: u32) -> Record {
    Record::new(T(b"SNCT"), object)
        .sub(T(b"FNAM"), flags.to_le_bytes())
        .sub(T(b"VNAM"), u16::MAX.to_le_bytes())
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(descriptor());
    plugin.push_top(
        category(CATEGORY, 2)
            .sub(T(b"UNAM"), 32768_u16.to_le_bytes())
            .sub(T(b"PNAM"), PARENT.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"STAT"), BASE));
    plugin.push_top(Record::new(T(b"STAT"), OTHER_BASE));
    plugin.push_top(Record::new(T(b"REFR"), SOURCE).sub(T(b"NAME"), BASE.to_le_bytes()));
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

fn condition(function: u16, parameter: u32, value: f32, run_on: u32) -> [u8; 32] {
    let mut data = [0; 32];
    data[4..8].copy_from_slice(&value.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data[20..24].copy_from_slice(&run_on.to_le_bytes());
    data
}

fn patch(order: &mut LoadOrder, record: Record) {
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    plugin.push_top(record);
    order.add("Patch.esp", plugin).unwrap();
}

fn reordered_patch(order: &mut LoadOrder, mut record: Record) {
    let mut other = Plugin::new(FileHeader::default());
    other.push_top(Record::new(T(b"STAT"), BASE));
    order.add("Other.esm", other).unwrap();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Other.esm".into(), "Skyrim.esm".into()],
        ..Default::default()
    });
    record.form_id = 0x0100_0800;
    for subrecord in &mut record.subrecords {
        if [T(b"GNAM"), T(b"ONAM")].contains(&subrecord.tag) {
            let raw = u32::from_le_bytes(subrecord.data[..4].try_into().unwrap());
            subrecord.data = (raw | 0x0100_0000).to_le_bytes().to_vec();
        }
    }
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
fn conditional_descriptors_require_runtime_context() {
    let mut order = order();
    patch(&mut order, descriptor().sub(T(b"CTDA"), [0; 32]));
    let error = prepare(&order, id(SOUND), [0.5; 3], &SoundSettings::default()).unwrap_err();
    assert!(error.to_string().contains("requires condition context"));
}

#[test]
fn conditional_descriptors_select_the_source_matching_branch() {
    let mut order = order();
    reordered_patch(
        &mut order,
        descriptor().sub(T(b"CTDA"), condition(72, 0x0100_0B01, 1.0, 0)),
    );
    let order = std::sync::Arc::new(order);
    let game = Game::from_shared_order(order.clone());
    let mut world = game.world_mut();
    assert_eq!(world.order().winner_ref(id(SOUND)).unwrap().file, 2);
    let playback = prepare_for(
        &order,
        &mut world,
        &game.vm,
        id(SOUND),
        id(SOURCE),
        None,
        [0.5; 3],
        &SoundSettings::default(),
    )
    .unwrap();
    assert_eq!(playback.data_path, "sound/fx/second.wav");
}

#[test]
fn conditional_descriptors_skip_nonmatching_branches() {
    let mut order = order();
    patch(
        &mut order,
        descriptor().sub(T(b"CTDA"), condition(72, OTHER_BASE, 1.0, 0)),
    );
    let order = std::sync::Arc::new(order);
    let game = Game::from_shared_order(order.clone());
    let mut world = game.world_mut();
    let error = prepare_for(
        &order,
        &mut world,
        &game.vm,
        id(SOUND),
        id(SOURCE),
        None,
        [0.5; 3],
        &SoundSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("do not match"));
}

#[test]
fn conditional_descriptors_reject_missing_target_context() {
    let mut order = order();
    patch(
        &mut order,
        descriptor().sub(T(b"CTDA"), condition(72, BASE, 1.0, 1)),
    );
    let order = std::sync::Arc::new(order);
    let game = Game::from_shared_order(order.clone());
    let mut world = game.world_mut();
    let error = prepare_for(
        &order,
        &mut world,
        &game.vm,
        id(SOUND),
        id(SOURCE),
        None,
        [0.5; 3],
        &SoundSettings::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("run-on target 1"));
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
