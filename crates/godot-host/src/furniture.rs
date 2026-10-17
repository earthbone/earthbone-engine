// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(not(feature = "godot"), allow(dead_code))]

//! FURN placement uses nifly's FurniturePosition and xEdit's FNPR/NAM0 fields.
//! The existing NIF reader retains these extra blocks as opaque data. This
//! adapter reads their declared fields without adding a second NIF reader.

use soe_nif::blocks::Block;
use soe_nif::stream::Stream;
use soe_physics::{Iso, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Marker {
    pub offset: [f32; 3],
    pub heading: f32,
    pub animation: u16,
    pub entries: u16,
}

pub(crate) fn markers(file: &soe_nif::NifFile) -> soe_nif::Result<Vec<Marker>> {
    let mut markers = Vec::new();
    for (block, data) in file
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(block, value)| match value {
            Block::Unknown { type_name, data }
                if matches!(
                    type_name.as_str(),
                    "BSFurnitureMarker" | "BSFurnitureMarkerNode"
                ) =>
            {
                Some((block, data))
            }
            _ => None,
        })
    {
        if file.header.user_version < 12 {
            return Err(soe_nif::Error::InvalidData {
                block,
                reason: "furniture requires Skyrim marker fields".into(),
            });
        }
        let mut stream = Stream::new(data);
        stream.u32()?;
        let count = stream.counted(20)?;
        for _ in 0..count {
            let marker = Marker {
                offset: stream.vec3()?.to_array(),
                heading: stream.f32()?,
                animation: stream.u16()?,
                entries: stream.u16()?,
            };
            if !marker
                .offset
                .into_iter()
                .chain([marker.heading])
                .all(f32::is_finite)
            {
                return Err(soe_nif::Error::InvalidData {
                    block,
                    reason: "nonfinite furniture marker".into(),
                });
            }
            markers.push(marker);
        }
        if stream.remaining() != 0 {
            return Err(soe_nif::Error::InvalidData {
                block,
                reason: "trailing furniture marker bytes".into(),
            });
        }
    }
    Ok(markers)
}

impl Marker {
    pub(crate) fn placement(self, furniture: soe_game::world::RefPlacement) -> ([f32; 3], f32) {
        let transform = Iso::placement(furniture.position, furniture.rotation, furniture.scale);
        let position = transform.apply(Vec3::new(self.offset[0], self.offset[1], self.offset[2]));
        (
            [position.x, position.y, position.z],
            furniture.rotation[2] + self.heading,
        )
    }
}

pub(crate) struct Use {
    pub seat: (soe_esm::GlobalFormId, u16),
    pub position: [f32; 3],
    pub heading: f32,
    pub event: String,
}

pub(crate) fn prepare(
    world: &mut soe_game::World,
    vfs: &soe_bsa::Vfs,
    actor: soe_esm::GlobalFormId,
) -> Result<Option<Use>, String> {
    use soe_esm::Tag;
    let Some(seat) = world.furniture_seat(actor) else {
        return Ok(None);
    };
    let ready = world
        .actor_ref(actor)
        .is_some_and(|state| state.ai.arrived || state.ai.destination.is_none());
    if !ready {
        return Ok(None);
    }
    let placement = world
        .ref_placement(seat.0)
        .ok_or("missing furniture placement")?;
    let actor_position = world
        .ref_placement(actor)
        .ok_or("missing furniture actor")?
        .position;
    let base = world.base_of(seat.0).ok_or("missing furniture base")?;
    let record = world.record(base).ok_or("missing FURN record")?;
    let model = record
        .find(Tag(*b"MODL"))
        .map(soe_esm::cursor::zstring)
        .ok_or("missing furniture model")?;
    let path = format!("meshes/{}", model.replace('\\', "/"));
    let bytes = vfs
        .read(&path)
        .map_err(|error| format!("{path}: {error}"))?;
    let nif = soe_nif::parse(&bytes).map_err(|error| format!("{path}: {error}"))?;
    let marker = markers(&nif)
        .map_err(|error| format!("{path}: {error}"))?
        .get(usize::from(seat.1))
        .copied()
        .ok_or("furniture marker index absent from model")?;
    let flags = record
        .all(Tag(*b"FNPR"))
        .nth(usize::from(seat.1))
        .filter(|data| data.len() == 4)
        .ok_or("missing furniture entry permissions")?;
    let animation = u16::from_le_bytes([flags[0], flags[1]]);
    let mut entries = marker.entries & u16::from_le_bytes([flags[2], flags[3]]);
    let mut index = None;
    for sub in &record.subrecords {
        if sub.tag == Tag(*b"ENAM") && sub.data.len() == 4 {
            index = Some(u32::from_le_bytes([
                sub.data[0],
                sub.data[1],
                sub.data[2],
                sub.data[3],
            ]));
        }
        if sub.tag == Tag(*b"NAM0") && index == Some(u32::from(seat.1)) && sub.data.len() == 4 {
            entries &= !u16::from_le_bytes([sub.data[2], sub.data[3]]);
        }
    }
    let (position, heading) = marker.placement(placement);
    let (sin, cos) = heading.sin_cos();
    let toward_actor = [
        actor_position[0] - position[0],
        actor_position[1] - position[1],
    ];
    let entry = [
        (1, [sin, cos]),
        (2, [-sin, -cos]),
        (4, [cos, -sin]),
        (8, [-cos, sin]),
        (16, [0.0, 0.0]),
    ]
    .into_iter()
    .filter(|(bit, _)| entries & bit != 0)
    .max_by(|a, b| {
        (toward_actor[0] * a.1[0] + toward_actor[1] * a.1[1])
            .total_cmp(&(toward_actor[0] * b.1[0] + toward_actor[1] * b.1[1]))
    })
    .map(|(bit, _)| bit)
    .ok_or("no permitted furniture entry point")?;
    let event = world
        .furniture_entry_idle(actor, animation, entry)
        .ok_or("no matching furniture IDLE entry")?;
    Ok(Some(Use {
        seat,
        position,
        heading,
        event,
    }))
}

#[derive(Default)]
pub(crate) struct ActorUse {
    seat: Option<(soe_esm::GlobalFormId, u16)>,
    exiting: bool,
}

impl ActorUse {
    pub(crate) fn sync(
        &mut self,
        world: &mut soe_game::World,
        vfs: &soe_bsa::Vfs,
        actor: soe_esm::GlobalFormId,
        vm: &mut soe_hkx::interp::Interpreter<'_>,
    ) -> Result<Vec<String>, String> {
        let mut events = Vec::new();
        let seat = world.furniture_seat(actor);
        if seat != self.seat {
            if let Some(use_) = prepare(world, vfs, actor)? {
                if let Some(state) = world.actor(actor) {
                    if state.weapon_drawn {
                        events.push("Unequip".to_owned());
                    }
                    state.weapon_drawn = false;
                    state.sneaking = false;
                    state.swimming = false;
                }
                world.set_host_actor_pose(actor, use_.position, [0.0, 0.0, use_.heading]);
                vm.variables.set_by_name("Speed", 0.0);
                events.push("moveStop".to_owned());
                events.push(use_.event);
                self.seat = Some(use_.seat);
                self.exiting = false;
            } else if seat.is_none() && self.seat.take().is_some() {
                events.push("IdleFurnitureExit".to_owned());
                self.exiting = false;
            }
        }
        if self.seat.is_some()
            && !self.exiting
            && world
                .actor_ref(actor)
                .is_some_and(|state| state.sit_wants_to_stand)
        {
            self.exiting = true;
        }
        Ok(events)
    }

    pub(crate) fn finish(
        &mut self,
        world: &mut soe_game::World,
        actor: soe_esm::GlobalFormId,
        vm: &soe_hkx::interp::Interpreter<'_>,
    ) {
        if self.exiting
            && vm
                .variables
                .index("isInFurniture")
                .and_then(|index| vm.variables.word(index))
                == Some(0)
        {
            world.stand_from_furniture(actor);
            self.seat = None;
            self.exiting = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_source::fixture::{Buf, NifWriter};

    fn file() -> soe_nif::NifFile {
        let mut writer = NifWriter::new(100);
        let mut bytes = Buf::default();
        bytes.u32(u32::MAX).u32(2);
        for (offset, heading, kind, entries) in [
            ([10.0, 20.0, 30.0], 0.5, 1, 5),
            ([-25.0, 8.0, 4.0], -0.75, 2, 8),
        ] {
            bytes.floats(&offset).f32(heading).u16(kind).u16(entries);
        }
        writer.add("BSFurnitureMarkerNode", bytes.0);
        soe_nif::parse(&writer.finish()).unwrap()
    }

    #[test]
    fn r4_furniture_markers_retain_each_offset_heading_type_and_entry() {
        let markers = markers(&file()).unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(
            markers[1],
            Marker {
                offset: [-25.0, 8.0, 4.0],
                heading: -0.75,
                animation: 2,
                entries: 8
            }
        );
        let placement = soe_game::world::RefPlacement {
            position: [100.0, 200.0, 300.0],
            rotation: [0.0, 0.0, std::f32::consts::FRAC_PI_2],
            scale: 2.0,
            visible: true,
        };
        let (position, heading) = markers[0].placement(placement);
        assert!((position[0] - 140.0).abs() < 0.001);
        assert!((position[1] - 180.0).abs() < 0.001);
        assert_eq!(position[2], 360.0);
        assert_eq!(heading, placement.rotation[2] + 0.5);
    }

    #[test]
    fn r4_furniture_markers_reject_truncation_and_invalid_numbers() {
        let file = file();
        let Block::Unknown { data, .. } = &file.blocks[0] else {
            panic!("marker block")
        };
        for length in 0..data.len() {
            let mut truncated = file.clone();
            let Block::Unknown { data, .. } = &mut truncated.blocks[0] else {
                panic!("marker block")
            };
            data.truncate(length);
            assert!(markers(&truncated).is_err(), "truncation at {length}");
        }
        let mut invalid = file;
        let Block::Unknown { data, .. } = &mut invalid.blocks[0] else {
            panic!("marker block")
        };
        data[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(markers(&invalid).is_err());
    }
}
