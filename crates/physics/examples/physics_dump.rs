// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Print the colliders of one mesh, either a loose file or a path inside
//! an archive: `physics_dump <file.nif>` or
//! `physics_dump <archive.bsa> <path/in/archive>`.
//! With `--raw`, also the trailing bytes of every rigid body, for layout
//! checks against nif.xml.

use std::process::ExitCode;

use soe_nif::blocks::Block;
use soe_physics::bhk::{self, Bhk};
use soe_physics::{collect, Shape};

fn describe(s: &Shape, depth: usize) {
    let pad = "  ".repeat(depth);
    match s {
        Shape::Material { id, shape } => {
            println!("{pad}material {id}");
            describe(shape, depth + 1);
        }
        Shape::Box { half_extents } => println!("{pad}box half extents {half_extents:?}"),
        Shape::Sphere { radius } => println!("{pad}sphere radius {radius}"),
        Shape::Capsule { a, b, radius } => println!("{pad}capsule {a:?} to {b:?} radius {radius}"),
        Shape::ConvexHull { points } => println!("{pad}convex hull, {} points", points.len()),
        Shape::TriangleMesh {
            vertices,
            triangles,
            ..
        } => println!(
            "{pad}triangle mesh, {} vertices, {} triangles",
            vertices.len(),
            triangles.len()
        ),
        Shape::Compound(children) => {
            println!("{pad}compound of {}", children.len());
            for (t, c) in children {
                println!("{pad}  at {:?}", t.translation);
                describe(c, depth + 2);
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let raw = args.iter().any(|a| a == "--raw");
    let args: Vec<&String> = args.iter().filter(|a| *a != "--raw").collect();
    let bytes = match args.as_slice() {
        [file] => std::fs::read(file).expect("file reads"),
        [archive, inner] => {
            let a = soe_bsa::Archive::open(std::path::Path::new(archive)).expect("archive opens");
            a.read(inner).expect("path is in the archive")
        }
        _ => {
            eprintln!("usage: dump [--raw] <file.nif> | <archive.bsa> <path/in/archive>");
            return ExitCode::FAILURE;
        }
    };
    let file = soe_nif::parse(&bytes).expect("nif parses");
    for (i, b) in file.blocks.iter().enumerate() {
        let Block::Collision { type_name, data } = b else {
            continue;
        };
        match bhk::decode(type_name, data, &file.header) {
            Ok(Bhk::RigidBody(rb)) => {
                println!(
                    "block {i} {type_name}: shape {:?} layer {:?} motion {:?} mass {} friction {} restitution {} translation {:?} rotation norm {:.4}",
                    rb.shape,
                    rb.layer,
                    rb.motion,
                    rb.mass,
                    rb.friction,
                    rb.restitution,
                    rb.translation,
                    rb.rotation.norm()
                );
                if raw {
                    let start = data.len().saturating_sub(48);
                    println!("  {} bytes, last 48: {:?}", data.len(), &data[start..]);
                }
            }
            Ok(Bhk::Other(t)) => println!("block {i} {t}: not decoded"),
            Ok(other) => println!("block {i} {type_name}: {}", summary(&other)),
            Err(e) => println!("block {i} {type_name}: error {e}"),
        }
    }
    let c = collect(&file).expect("collects");
    for col in &c.colliders {
        println!(
            "collider on {:?} layer {:?} motion {:?} at {:?}",
            col.node_name, col.layer, col.motion, col.transform.translation
        );
        describe(&col.shape, 1);
    }
    for (r, why) in &c.skipped {
        println!("skipped {r:?}: {why}");
    }
    ExitCode::SUCCESS
}

fn summary(b: &Bhk) -> String {
    match b {
        Bhk::CollisionObject { body, .. } => format!("collision object, body {body:?}"),
        Bhk::Box { half_extents, .. } => format!("box {half_extents:?}"),
        Bhk::Sphere { radius, .. } => format!("sphere {radius}"),
        Bhk::Capsule { first, second, .. } => format!("capsule {first:?} {second:?}"),
        Bhk::ConvexVertices { vertices, .. } => format!("convex, {} vertices", vertices.len()),
        Bhk::Transform { shape, .. } => format!("transform of {shape:?}"),
        Bhk::List { children, .. } => format!("list of {}", children.len()),
        Bhk::MoppBvTree { shape, .. } => format!("mopp over {shape:?}"),
        Bhk::CompressedMesh { data, .. } => format!("compressed mesh, data {data:?}"),
        Bhk::CompressedMeshData(d) => format!(
            "compressed mesh data, {} chunks, {} big triangles",
            d.chunks.len(),
            d.big_triangles.len()
        ),
        Bhk::RigidBody(_) | Bhk::Other(_) => String::new(),
    }
}
