#![allow(clippy::expect_used, clippy::panic)]

use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions, SurfaceArrays};

fn feed(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn feed_values(hash: &mut u64, tag: u32, values: impl IntoIterator<Item = u32>) {
    feed(hash, tag);
    let values: Vec<_> = values.into_iter().collect();
    feed(hash, values.len() as u32);
    for value in values {
        feed(hash, value);
    }
}

fn host_hash(surfaces: &[SurfaceArrays]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    feed(&mut hash, surfaces.len() as u32);
    for surface in surfaces {
        feed_values(
            &mut hash,
            1,
            surface
                .positions
                .iter()
                .flatten()
                .map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            2,
            surface
                .normals
                .iter()
                .flatten()
                .map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            3,
            surface.tangents.iter().map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            4,
            surface.uvs.iter().flatten().map(|value| value.to_bits()),
        );
        feed_values(
            &mut hash,
            5,
            surface.indices.iter().map(|value| *value as u32),
        );
        feed_values(
            &mut hash,
            6,
            surface.bones.iter().map(|value| *value as u32),
        );
        feed_values(
            &mut hash,
            7,
            surface.weights.iter().map(|value| value.to_bits()),
        );
        feed(&mut hash, 8);
        for value in [
            surface.bounds.min.x,
            surface.bounds.min.y,
            surface.bounds.min.z,
            surface.bounds.max.x,
            surface.bounds.max.y,
            surface.bounds.max.z,
        ] {
            feed(&mut hash, value.to_bits());
        }
    }
    hash
}

#[test]
fn tst_02_host_hash_distinguishes_position_and_normal_array_boundaries() {
    let mesh = MeshData {
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        indices: vec![0, 1, 2],
        material: MaterialDesc::default(),
        ..MeshData::default()
    };
    let original = build_surface(&mesh, MeshOptions::default()).expect("triangle mesh builds");
    assert_eq!(original.positions.len(), 3);
    assert_eq!(original.normals.len(), 3);

    let mut repartitioned = original.clone();
    let moved = repartitioned
        .positions
        .pop()
        .expect("surface includes positions");
    repartitioned.normals.insert(0, moved);

    assert_ne!(
        host_hash(std::slice::from_ref(&original)),
        host_hash(&[repartitioned]),
        "host result hash must distinguish fields with different array boundaries"
    );
}
