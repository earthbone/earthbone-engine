// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! RND-14's named SSE emitter fixture. The bytes follow nifxml's
//! `NiParticleSystem`, `NiPSysSphereEmitter`, and `NiPSysEmitterCtlr`
//! layouts, cross-checked against nifly's `Particles.cpp`.

mod common;

use common::{Buf, NifWriter, IDENTITY};
use soe_nif::particle::Timeline;

fn named_emitter_fixture(lifetime: f32, variation: f32, visible: bool) -> Vec<u8> {
    let mut writer = NifWriter::new(100);
    let system_ref = writer.reserve("NiParticleSystem");
    let emitter_ref = writer.reserve("NiPSysSphereEmitter");
    let controller_ref = writer.reserve("NiPSysEmitterCtlr");
    let interp_ref = writer.reserve("NiFloatInterpolator");
    let data_ref = writer.reserve("NiPSysData");
    let visibility_ref = writer.reserve("NiBoolInterpolator");

    let mut system = Buf::default();
    writer.av_object(
        &mut system,
        "RND14SmokeSystem",
        [10.0, 20.0, 30.0],
        IDENTITY,
        1.0,
        -1,
        controller_ref,
    );
    system.vec3([0.0; 3]).f32(100.0);
    system.i32(-1).i32(-1).i32(-1).u64(0);
    for _ in 0..4 {
        system.u16(0);
    }
    system.i32(data_ref).bool(true).refs(&[emitter_ref]);
    writer.fill(system_ref, system.0);

    let mut emitter = Buf::default();
    emitter
        .u32(writer.string("RND14SmokeEmitter"))
        .u32(1000)
        .i32(system_ref)
        .bool(true);
    emitter.floats(&[0.0; 6]);
    emitter.floats(&[1.0, 0.5, 0.25, 1.0]);
    emitter.f32(2.0).f32(0.0).f32(lifetime).f32(variation);
    emitter.i32(-1).f32(1.0);
    writer.fill(emitter_ref, emitter.0);

    let mut controller =
        Buf(writer.transform_controller(-1, 0xC, 1.0, 0.0, 0.0, 10.0, system_ref, interp_ref));
    controller
        .u32(writer.string("RND14SmokeEmitter"))
        .i32(visibility_ref);
    writer.fill(controller_ref, controller.0);

    let mut interp = Buf::default();
    interp.f32(4.0).i32(-1);
    writer.fill(interp_ref, interp.0);

    let mut data = Buf::default();
    data.i32(0).u16(16);
    writer.fill(data_ref, data.0);
    let mut visibility = Buf::default();
    visibility.u8(u8::from(visible)).i32(-1);
    writer.fill(visibility_ref, visibility.0);
    writer.roots.push(system_ref);
    writer.finish()
}

#[test]
fn rnd_14_named_emitter_spawns_at_authored_rate() {
    let file = soe_nif::parse(&named_emitter_fixture(0.5, 0.0, true)).unwrap();
    let emitters = file.particle_emitters().unwrap();
    assert_eq!(emitters.len(), 1);
    assert_eq!(emitters[0].name, "RND14SmokeEmitter");
    assert_eq!(emitters[0].system_name, "RND14SmokeSystem");
    assert_eq!(emitters[0].rate.at(0.0), 4.0);
    assert_eq!(emitters[0].rate.time.target, soe_nif::types::BlockRef(0));
    assert_eq!(emitters[0].capacity, 16);
    let mut timeline = Timeline::new(emitters[0].clone());
    timeline.advance(0.5);
    assert_eq!(timeline.emitted(), 2);
    assert_eq!(timeline.alive(), 2);
    assert_eq!(timeline.particles()[0].position, [10.0, 20.0, 30.0]);
}

#[test]
fn rnd_14_named_emitter_expires_at_authored_lifetime() {
    let file = soe_nif::parse(&named_emitter_fixture(0.5, 0.0, true)).unwrap();
    let mut timeline = Timeline::new(file.particle_emitters().unwrap().remove(0));
    timeline.advance(0.5);
    timeline.advance(0.5);
    assert_eq!(timeline.emitted(), 4);
    assert_eq!(timeline.alive(), 2);
    assert_eq!(timeline.particles()[0].born_at, 0.75);
    assert_eq!(timeline.particles()[1].born_at, 1.0);
}

#[test]
fn rnd_14_nonpositive_lifetime_is_rejected() {
    let file = soe_nif::parse(&named_emitter_fixture(0.0, 0.0, true)).unwrap();
    assert!(matches!(
        file.particle_emitters(),
        Err(soe_nif::Error::InvalidData { .. })
    ));
}

#[test]
fn rnd_14_write_named_godot_fixture() {
    let bytes = named_emitter_fixture(0.5, 0.0, true);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/godot-check/fixtures/rnd_14_emitter.nif");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn rnd_14_inactive_visibility_channel_suppresses_births() {
    let file = soe_nif::parse(&named_emitter_fixture(0.5, 0.0, false)).unwrap();
    let mut timeline = Timeline::new(file.particle_emitters().unwrap().remove(0));
    timeline.advance(1.0);
    assert_eq!(timeline.emitted(), 0);
    assert_eq!(timeline.alive(), 0);
}

#[test]
fn rnd_14_authored_lifetime_variation_changes_expiry() {
    let file = soe_nif::parse(&named_emitter_fixture(0.5, 0.5, true)).unwrap();
    let mut timeline = Timeline::new(file.particle_emitters().unwrap().remove(0));
    timeline.advance(0.5);
    let lifetimes: Vec<_> = timeline.particles().iter().map(|p| p.lifetime).collect();
    assert_eq!(lifetimes.len(), 2);
    assert!(lifetimes.iter().all(|life| *life >= 0.5 && *life < 1.0));
    assert_ne!(lifetimes[0], lifetimes[1]);
}

#[test]
fn rnd_14_particle_data_quota_caps_live_births() {
    let file = soe_nif::parse(&named_emitter_fixture(10.0, 0.0, true)).unwrap();
    let mut emitter = file.particle_emitters().unwrap().remove(0);
    emitter.capacity = 1;
    let mut timeline = Timeline::new(emitter);
    timeline.advance(1.0);
    assert_eq!(timeline.emitted(), 1);
    assert_eq!(timeline.alive(), 1);
}
