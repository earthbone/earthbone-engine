// SPDX-License-Identifier: GPL-3.0-or-later

//! NAVM: a navmesh, one per cell in the temporary children group.
//!
//! The NVNM subrecord is a single packed structure. Layout, from xEdit's
//! `wbNVNM` and OpenMW's `NVNMstruct::load`:
//!
//! ```text
//! version:u32 location_crc:u32 world:u32
//! if world == 0: cell:u32 else grid_y:i16 grid_x:i16
//! count:u32 vertices[count]   x y z as f32
//! count:u32 triangles[count]  v0 v1 v2 as u16, e01 e12 e20 as i16, flags:u16 cover:u16
//! count:u32 edge_links[count] kind:u32 navmesh:u32 triangle:i16
//! count:u32 door_links[count] triangle:i16 crc:u32 door:u32
//! count:u32 cover_triangles[count] as i16
//! divisor:u32 size_x:f32 size_y:f32 min[3] max[3] as f32
//! cells[divisor * divisor] each count:u32 triangles[count] as i16
//! ```

use crate::cursor::Fields;
use crate::error::{Error, Result};
use crate::plugin::Record;
use crate::tag::{self, Tag};

use super::{form_id_list, u16_list};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    /// Interior: the cell's FormID.
    Cell(u32),
    /// Exterior: the worldspace and the cell grid position.
    Grid { world: u32, x: i16, y: i16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    pub vertices: [u16; 3],
    /// Adjacent triangle across edge 0-1, 1-2, 2-0, or -1 for none.
    pub edges: [i16; 3],
    pub flags: u16,
    pub cover_flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdgeLink {
    pub kind: u32,
    pub navmesh: u32,
    pub triangle: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoorLink {
    pub triangle: i16,
    pub crc: u32,
    pub door: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavGrid {
    pub divisor: u32,
    pub size: [f32; 2],
    pub min: [f32; 3],
    pub max: [f32; 3],
    /// divisor squared cells, each listing the triangles that touch it.
    pub cells: Vec<Vec<i16>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    pub version: u32,
    pub location_crc: u32,
    pub parent: Parent,
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<Triangle>,
    pub edge_links: Vec<EdgeLink>,
    pub door_links: Vec<DoorLink>,
    pub cover_triangles: Vec<i16>,
    pub grid: NavGrid,
}

/// Counts in a navmesh are bounded by the subrecord size, so a count that
/// could not fit in the remaining bytes is a malformed file, not an
/// allocation request.
fn counted<T>(f: &mut Fields<'_>, item_size: usize, what: &str, mut read: impl FnMut(&mut Fields<'_>) -> Result<T>) -> Result<Vec<T>> {
    let count = f.u32()? as usize;
    if count.saturating_mul(item_size) > f.remaining() {
        return Err(Error::Invalid(format!(
            "NVNM declares {count} {what} but only {} bytes remain",
            f.remaining()
        )));
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(read(f)?);
    }
    Ok(out)
}

impl Geometry {
    pub fn parse(record: Tag, data: &[u8]) -> Result<Geometry> {
        let mut f = Fields::new(record, Tag::new(b"NVNM"), data);
        let version = f.u32()?;
        let location_crc = f.u32()?;
        let world = f.u32()?;
        let parent = if world == 0 {
            Parent::Cell(f.u32()?)
        } else {
            let y = f.i16()?;
            let x = f.i16()?;
            Parent::Grid { world, x, y }
        };
        let vertices = counted(&mut f, 12, "vertices", |f| Ok([f.f32()?, f.f32()?, f.f32()?]))?;
        let triangles = counted(&mut f, 16, "triangles", |f| {
            Ok(Triangle {
                vertices: [f.u16()?, f.u16()?, f.u16()?],
                edges: [f.i16()?, f.i16()?, f.i16()?],
                flags: f.u16()?,
                cover_flags: f.u16()?,
            })
        })?;
        let vertex_count = vertices.len();
        if let Some(t) = triangles.iter().find(|t| t.vertices.iter().any(|v| usize::from(*v) >= vertex_count)) {
            return Err(Error::Invalid(format!(
                "NVNM triangle {:?} indexes past its {vertex_count} vertices",
                t.vertices
            )));
        }
        let edge_links = counted(&mut f, 10, "edge links", |f| {
            Ok(EdgeLink {
                kind: f.u32()?,
                navmesh: f.u32()?,
                triangle: f.i16()?,
            })
        })?;
        let door_links = counted(&mut f, 10, "door links", |f| {
            Ok(DoorLink {
                triangle: f.i16()?,
                crc: f.u32()?,
                door: f.u32()?,
            })
        })?;
        let cover_triangles = counted(&mut f, 2, "cover triangles", |f| f.i16())?;
        let divisor = f.u32()?;
        let size = [f.f32()?, f.f32()?];
        let min = [f.f32()?, f.f32()?, f.f32()?];
        let max = [f.f32()?, f.f32()?, f.f32()?];
        let cell_count = (divisor as usize).saturating_mul(divisor as usize);
        if cell_count.saturating_mul(4) > f.remaining() {
            return Err(Error::Invalid(format!(
                "NVNM grid divisor {divisor} needs {cell_count} cells but only {} bytes remain",
                f.remaining()
            )));
        }
        let mut cells = Vec::with_capacity(cell_count);
        for _ in 0..cell_count {
            cells.push(counted(&mut f, 2, "grid cell triangles", |f| f.i16())?);
        }
        Ok(Geometry {
            version,
            location_crc,
            parent,
            vertices,
            triangles,
            edge_links,
            door_links,
            cover_triangles,
            grid: NavGrid {
                divisor,
                size,
                min,
                max,
                cells,
            },
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.location_crc.to_le_bytes());
        match self.parent {
            Parent::Cell(c) => {
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&c.to_le_bytes());
            }
            Parent::Grid { world, x, y } => {
                out.extend_from_slice(&world.to_le_bytes());
                out.extend_from_slice(&y.to_le_bytes());
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.vertices.len() as u32).to_le_bytes());
        for v in &self.vertices {
            for c in v {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        out.extend_from_slice(&(self.triangles.len() as u32).to_le_bytes());
        for t in &self.triangles {
            for v in t.vertices {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for e in t.edges {
                out.extend_from_slice(&e.to_le_bytes());
            }
            out.extend_from_slice(&t.flags.to_le_bytes());
            out.extend_from_slice(&t.cover_flags.to_le_bytes());
        }
        out.extend_from_slice(&(self.edge_links.len() as u32).to_le_bytes());
        for e in &self.edge_links {
            out.extend_from_slice(&e.kind.to_le_bytes());
            out.extend_from_slice(&e.navmesh.to_le_bytes());
            out.extend_from_slice(&e.triangle.to_le_bytes());
        }
        out.extend_from_slice(&(self.door_links.len() as u32).to_le_bytes());
        for d in &self.door_links {
            out.extend_from_slice(&d.triangle.to_le_bytes());
            out.extend_from_slice(&d.crc.to_le_bytes());
            out.extend_from_slice(&d.door.to_le_bytes());
        }
        out.extend_from_slice(&(self.cover_triangles.len() as u32).to_le_bytes());
        for c in &self.cover_triangles {
            out.extend_from_slice(&c.to_le_bytes());
        }
        out.extend_from_slice(&self.grid.divisor.to_le_bytes());
        for v in self.grid.size.iter().chain(&self.grid.min).chain(&self.grid.max) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for cell in &self.grid.cells {
            out.extend_from_slice(&(cell.len() as u32).to_le_bytes());
            for t in cell {
                out.extend_from_slice(&t.to_le_bytes());
            }
        }
        out
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NavMesh {
    pub form_id: u32,
    pub record_flags: u32,
    pub editor_id: Option<String>,
    pub geometry: Option<Geometry>,
    /// ONAM: base objects the navmesh was generated against.
    pub base_objects: Vec<u32>,
    pub preferred_connectors: Vec<u16>,
    pub non_connectors: Vec<u16>,
    pub cell: Option<u32>,
    pub world: Option<u32>,
}

impl NavMesh {
    pub fn from_record(rec: &Record) -> Result<NavMesh> {
        debug_assert_eq!(rec.tag, tag::NAVM);
        let geometry = match rec.find(Tag::new(b"NVNM")) {
            None => None,
            Some(d) => Some(Geometry::parse(rec.tag, d)?),
        };
        Ok(NavMesh {
            form_id: rec.form_id,
            record_flags: rec.flags,
            editor_id: rec.editor_id(),
            geometry,
            base_objects: form_id_list(rec, Tag::new(b"ONAM"))?,
            preferred_connectors: u16_list(rec, Tag::new(b"PNAM"))?,
            non_connectors: u16_list(rec, Tag::new(b"NNAM"))?,
            cell: rec.placement.cell,
            world: rec.placement.world,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Geometry {
        Geometry {
            version: 12,
            location_crc: 0xDEADBEEF,
            parent: Parent::Grid { world: 0x3C, x: 4, y: -2 },
            vertices: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]],
            triangles: vec![
                Triangle { vertices: [0, 1, 2], edges: [-1, -1, 1], flags: 0, cover_flags: 0 },
                Triangle { vertices: [0, 2, 3], edges: [0, -1, -1], flags: 0, cover_flags: 0 },
            ],
            edge_links: vec![EdgeLink { kind: 2, navmesh: 0x7000, triangle: 0 }],
            door_links: vec![DoorLink { triangle: 1, crc: 0x9F5C46A1, door: 0x8000 }],
            cover_triangles: vec![1],
            grid: NavGrid {
                divisor: 2,
                size: [50.0, 50.0],
                min: [0.0, 0.0, 0.0],
                max: [100.0, 100.0, 0.0],
                cells: vec![vec![0, 1], vec![0], vec![1], vec![]],
            },
        }
    }

    #[test]
    fn a_navmesh_round_trips_and_reads_its_parent() {
        let g = square();
        let bytes = g.to_bytes();
        let back = Geometry::parse(tag::NAVM, &bytes).unwrap();
        assert_eq!(back, g);
        let rec = Record::new(tag::NAVM, 0x6000)
            .sub(b"NVNM", bytes)
            .sub(b"ONAM", 0x800u32.to_le_bytes())
            .sub(b"PNAM", [1u16.to_le_bytes(), 2u16.to_le_bytes()].concat());
        let n = NavMesh::from_record(&rec).unwrap();
        assert_eq!(n.geometry.unwrap().parent, Parent::Grid { world: 0x3C, x: 4, y: -2 });
        assert_eq!(n.base_objects, vec![0x800]);
        assert_eq!(n.preferred_connectors, vec![1, 2]);
        let mut interior = square();
        interior.parent = Parent::Cell(0x9999);
        assert_eq!(Geometry::parse(tag::NAVM, &interior.to_bytes()).unwrap().parent, Parent::Cell(0x9999));
    }

    #[test]
    fn absurd_counts_are_refused_before_allocating() {
        let mut bytes = square().to_bytes();
        // The vertex count sits after version, crc, world, grid (16 bytes).
        bytes[16..20].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let err = Geometry::parse(tag::NAVM, &bytes).unwrap_err();
        assert!(err.to_string().contains("declares 4294967295 vertices"), "{err}");
        let mut bad_index = square();
        bad_index.triangles[0].vertices[2] = 9;
        let err = Geometry::parse(tag::NAVM, &bad_index.to_bytes()).unwrap_err();
        assert!(err.to_string().contains("indexes past"), "{err}");
        let mut bad_grid = square();
        bad_grid.grid.divisor = 1000;
        bad_grid.grid.cells.clear();
        let err = Geometry::parse(tag::NAVM, &bad_grid.to_bytes()).unwrap_err();
        assert!(err.to_string().contains("divisor 1000"), "{err}");
        // Cut inside the vertex count field: a plain size error. Cut after
        // it: the count no longer fits and is refused as absurd.
        let full = square().to_bytes();
        assert!(matches!(Geometry::parse(tag::NAVM, &full[..18]).unwrap_err(), Error::SubrecordSize { .. }));
        assert!(matches!(Geometry::parse(tag::NAVM, &full[..30]).unwrap_err(), Error::Invalid(_)));
    }
}
