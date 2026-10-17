// SPDX-License-Identifier: GPL-3.0-or-later

use crate::data_source::DataSource;
use soe_nif::object_animation::{ObjectAnimation, PropertyChannel};
use std::path::Path;

#[test]
fn r4_installed_giant_fire_authored_clock_survey() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let path = "meshes/clutter/giant/giantcampfire01burning.nif";
    let file = soe_nif::parse(&source.vfs().read(path).unwrap()).unwrap();
    for (index, block) in file.blocks.iter().enumerate() {
        if let soe_nif::blocks::Block::Unknown { type_name, data } = block {
            if type_name.starts_with("NiPSys")
                || type_name == "BSPSysScaleModifier"
                || type_name == "NiColorData"
            {
                eprintln!("R4 SOURCE {index} {type_name} {data:02x?}");
            }
        }
    }
    let animation = ObjectAnimation::from_file(&file).unwrap();
    eprintln!(
        "R4 FIRE authored sequences={:?} emitters={:?}",
        animation.sequence_names().collect::<Vec<_>>(),
        file.particle_emitters()
    );
    let mesh = file
        .meshes()
        .unwrap()
        .into_iter()
        .find(|mesh| {
            mesh.material
                .diffuse()
                .is_some_and(|path| path.to_ascii_lowercase().contains("fxfirescrolltile02"))
        })
        .unwrap();
    let tracks = crate::nif_effect_animation::for_shape(&file, mesh.block).unwrap();
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    eprintln!("R4 FIRE authored shape={} tracks={tracks:?}", mesh.name);
    assert_eq!(track.channel, 8);
    assert!((track.frequency - 1.0).abs() <= 1e-6);
    assert!(track.start.abs() <= 1e-6);
    assert!((track.stop - 1.3).abs() <= 1e-6);
    assert_eq!(track.keys.keys.len(), 2);
    let shader = soe_nif::object_animation::shader_ref(&file.blocks[mesh.block])
        .get()
        .unwrap();
    for seconds in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let pose = animation.sample(seconds, None);
        let value = pose
            .properties
            .iter()
            .find(|property| {
                property.target == shader && property.channel == PropertyChannel::VOffset
            })
            .unwrap()
            .value
            .x;
        eprintln!("R4 FIRE authored seconds={seconds} v_offset={value}");
        assert!((value - seconds / 1.3).abs() <= 1e-5);
    }
}

#[test]
fn installed_giant_camp_fire_clock_walkthrough() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let binary = std::env::var_os("SOE_GODOT_BIN").expect("installed renderer requires Godot");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join("target/r4-fire-regression");
    std::fs::create_dir_all(&directory).unwrap();
    let output = std::process::Command::new(binary)
        .arg("--path")
        .arg(root.join("godot"))
        .args([
            "--resolution",
            "640x360",
            "--script",
            "r4_camp_fire_probe.gd",
        ])
        .env("DISPLAY", ":99")
        .env("SKYRIM_DATA", data)
        .env("SOE_GODOT_TIMEOUT", "900")
        .env("SOE_CHARACTER_OUTPUT", &directory)
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::write(directory.join("test.log"), &log).unwrap();
    assert!(
        output.status.success()
            && log.contains("RESULT: PASS R4 camp fire diagnostic")
            && !log.contains("SCRIPT ERROR"),
        "{log}"
    );
}

#[test]
fn r4_installed_giant_fire_streams_all_authored_emitters() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let fire = soe_esm::GlobalFormId {
        file: 0,
        object: 0x040911,
    };
    let loaded = source.stream_reference(fire).unwrap();
    assert_eq!(loaded.particles.len(), 3);
    let mut rates: Vec<_> = loaded
        .particles
        .iter()
        .map(|p| p.emitter.timing.rate.value.round() as i32)
        .collect();
    rates.sort_unstable();
    assert_eq!(rates, [3, 24, 60]);
    for particle in &loaded.particles {
        eprintln!("R4 RENDER source={:?}", particle.emitter);
        #[cfg(feature = "godot")]
        std::fs::write(
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                "../../target/r4-particle-{}.gdshader",
                particle.emitter.block
            )),
            crate::godot_layer::particles::process_shader(&particle.emitter),
        )
        .unwrap();
        #[cfg(feature = "godot")]
        {
            let shader = crate::nif_effect::shader(
                &particle.material,
                particle.material.effect.as_ref().unwrap(),
            );
            std::fs::write(
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
                    "../../target/r4-particle-{}-draw.gdshader",
                    particle.emitter.block
                )),
                crate::godot_layer::particles::billboard_shader(&shader, &particle.emitter),
            )
            .unwrap();
        }
    }
    for particle in loaded.particles {
        assert_eq!(particle.reference, fire);
        assert!(
            particle.emitter.unsupported.is_empty(),
            "{:?}",
            particle.emitter.unsupported
        );
        assert!(particle
            .material
            .diffuse
            .as_ref()
            .is_some_and(|p| !p.is_empty()));
        assert!(particle.textures.diffuse.is_some());
        let expected_speed = match particle.emitter.timing.system_name.as_str() {
            "PSmoke" => 0.0,
            "PFireball" => 60.0,
            "PCloudForgeSparks" => 45.0,
            name => panic!("unexpected particle system {name}"),
        };
        assert_eq!(particle.emitter.velocity[0], expected_speed);
        assert!(particle.emitter.timing.capacity > 0);
    }
}
