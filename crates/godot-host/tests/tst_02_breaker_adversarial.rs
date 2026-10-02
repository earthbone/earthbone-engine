#![allow(clippy::panic)]

use soe_godot_host::mesh::{build_surface, MaterialDesc, MeshData, MeshOptions, SurfaceArrays};

fn feed(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn host_hash(surfaces: &[SurfaceArrays]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for surface in surfaces {
        for value in surface.positions.iter().flatten() {
            feed(&mut hash, value.to_bits());
        }
        for value in surface.normals.iter().flatten() {
            feed(&mut hash, value.to_bits());
        }
        for value in &surface.tangents {
            feed(&mut hash, value.to_bits());
        }
        for value in surface.uvs.iter().flatten() {
            feed(&mut hash, value.to_bits());
        }
        for value in &surface.indices {
            feed(&mut hash, *value as u32);
        }
        for value in &surface.bones {
            feed(&mut hash, *value as u32);
        }
        for value in &surface.weights {
            feed(&mut hash, value.to_bits());
        }
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
    let original = build_surface(&mesh, MeshOptions::default()).unwrap();
    assert_eq!(original.positions.len(), 3);
    assert_eq!(original.normals.len(), 3);

    let mut repartitioned = original.clone();
    let moved = repartitioned.positions.pop().unwrap();
    repartitioned.normals.insert(0, moved);

    assert_ne!(
        host_hash(std::slice::from_ref(&original)),
        host_hash(&[repartitioned]),
        "host result hash must distinguish fields with different array boundaries"
    );
}
