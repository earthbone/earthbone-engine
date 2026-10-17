// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn hst_52_installed_load_door_record_audit() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let clicked_id = GlobalFormId {
        file: 0,
        object: 0x56df3,
    };
    let clicked_owner = source.order().winner_ref(clicked_id).unwrap();
    let clicked = Refr::from_record(source.order().winner(clicked_id).unwrap()).unwrap();
    let clicked_base = source.order().resolve(clicked_owner.file, clicked.base);
    let clicked_base_record = source.order().winner(clicked_base).unwrap();
    println!("HST52 clicked={clicked_id} reference={clicked:?} base={clicked_base} editor={:?} model={:?}",
        clicked_base_record.editor_id(), clicked_base_record.find(Tag(*b"MODL")).map(String::from_utf8_lossy));
    assert_eq!(clicked_base_record.tag, Tag(*b"STAT"));
    assert!(clicked.teleport.is_none());
    let mut doors = std::collections::BTreeSet::new();
    for name in [
        "WhiterunBanneredMare",
        "WhiterunDragonsreach",
        "WhiterunHeimskrsHouse",
    ] {
        let cell = source.resolve_cell(name).unwrap();
        for &id in source.cell_index().references(cell) {
            let Some(record) = source.order().winner(id) else {
                continue;
            };
            if record.tag != Tag(*b"REFR") {
                continue;
            }
            let reference = Refr::from_record(record).unwrap();
            if let Some(teleport) = reference.teleport {
                let owner = source.order().winner_ref(id).unwrap();
                doors.insert(id);
                doors.insert(source.order().resolve(owner.file, teleport.door));
            }
        }
    }
    let cell = source.resolve_cell("WhiterunHeimskrsHouse").unwrap();
    let collision = source.collide_cell_id(cell).unwrap();
    use soe_physics::{PhysicsWorld, Vec3};
    let from = Vec3::new(-192.1351, -251.8091, 180.04);
    let target = Vec3::new(-190.9455, -310.0088, 160.6738);
    let direction = target - from;
    let to = from + direction * (192.0 / direction.length());
    if let Some(hit) = collision.world.raycast(from, to) {
        println!(
            "HST52 physics first={:?} point={:?} normal={:?} distance={}",
            collision.references.get(&hit.body),
            hit.point,
            hit.normal,
            (hit.point - from).length()
        );
    }
    for placed in &collision.placed {
        if [0x56df3, 0x16659].contains(&placed.reference.object) {
            println!(
                "HST52 collider={} node={} mass={} motion={:?} bounds={:?} triangles={}",
                placed.reference,
                placed.collider.node_name,
                placed.collider.mass,
                placed.collider.motion,
                placed.collider.aabb(),
                placed.collider.shape.triangle_count()
            );
        }
    }
    let mut visual_world = soe_physics::SimpleWorld::new();
    let mut visual_owners = std::collections::HashMap::new();
    for object in [0x56df3, 0x16659] {
        let id = GlobalFormId { file: 0, object };
        let prototype = &collision
            .placed
            .iter()
            .find(|placed| placed.reference == id)
            .unwrap()
            .collider;
        for surface in source.stream_reference(id).unwrap().surfaces {
            let animated_transform = surface.animation.as_ref().map(|animation| {
                let pose = crate::object_animation::DoorAnimation::new(
                    Arc::clone(&animation.asset),
                    false,
                )
                .map_or_else(|| animation.asset.sample(0.0, None), |door| door.pose());
                animation.placement.then(&pose.transforms[animation.block])
            });
            let vertices = surface
                .arrays
                .positions
                .iter()
                .map(|p| {
                    let mut point = NifVec3 {
                        x: p[0],
                        y: p[1],
                        z: p[2],
                    };
                    if let Some(transform) = surface.instance_transform {
                        point = Mat33 {
                            rows: transform.basis,
                        }
                        .apply(point);
                        point.x += transform.origin.x;
                        point.y += transform.origin.y;
                        point.z += transform.origin.z;
                    }
                    let p = crate::units::GodotPos::new(point.x, point.y, point.z).to_skyrim();
                    let point = NifVec3::new(p.x, p.y, p.z);
                    let point =
                        animated_transform.map_or(point, |transform| transform.apply(point));
                    Vec3::new(point.x, point.y, point.z)
                })
                .collect();
            let triangles: Vec<_> = surface
                .arrays
                .indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| {
                    [
                        u32::try_from(p[0]).unwrap(),
                        u32::try_from(p[1]).unwrap(),
                        u32::try_from(p[2]).unwrap(),
                    ]
                })
                .collect();
            let mut collider = prototype.clone();
            collider.transform = soe_physics::Iso::IDENTITY;
            collider.shape = soe_physics::Shape::TriangleMesh {
                radius: 0.0,
                vertices,
                materials: vec![None; triangles.len()],
                triangles,
            };
            visual_owners.insert(visual_world.add_static(&collider), id);
        }
    }
    let visible = visual_world.raycast(from, to).unwrap();
    println!(
        "HST52 visible first={:?} point={:?} distance={}",
        visual_owners.get(&visible.body),
        visible.point,
        (visible.point - from).length()
    );
    assert_eq!(
        visual_owners[&visible.body],
        GlobalFormId {
            file: 0,
            object: 0x16659
        }
    );
    for id in doors {
        let record = source.order().winner(id).unwrap();
        let owner = source.order().winner_ref(id).unwrap();
        let reference = Refr::from_record(record).unwrap();
        let base = source.order().resolve(owner.file, reference.base);
        let base_record = source.order().winner(base).unwrap();
        println!(
            "HST52 door={id} reference={reference:?} base={base} tag={} editor={:?} flags={:?} model={:?}",
            base_record.tag,
            base_record.editor_id(),
            base_record.find(Tag(*b"FNAM")),
            base_record.find(Tag(*b"MODL")).map(String::from_utf8_lossy)
        );
        assert_eq!(base_record.tag, Tag(*b"DOOR"));
    }
}
