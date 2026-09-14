// SPDX-License-Identifier: GPL-3.0-or-later

//! LAND heights into Terrain3D regions.
//!
//! A LAND record's VHGT is a 33 by 33 grid of deltas over one cell, rows
//! from south to north, columns from west to east, in eighths of a unit.
//! Terrain3D wants square regions of `region_size` vertices (a power of two
//! from 64 to 2048) at a fixed `vertex_spacing`, laid on a grid, heights as
//! an `Image` in `FORMAT_RF`. Keeping one vertex per LAND sample gives a
//! spacing of 128 units, 1.83 m, and a region of `cells_per_region` cells a
//! side, so 8 cells make the 256-vertex default region.
//!
//! The only subtle part is the axis flip. Skyrim's north is Godot's -Z, and
//! flipping a half-open interval turns `[a, b)` into `(-b, -a]`, one sample
//! off Godot's own `[-b, -a)` region grid. So a Godot region's rows come
//! from LAND rows 1 to 32 of its cells, where row 32 is the shared northern
//! edge, and the seam stays exact because every cell carries that edge.

use std::collections::HashMap;

use crate::streaming::CellCoord;
use crate::units::{CELL_METRES, LAND_HEIGHT_UNITS, LAND_SAMPLES, METRES_PER_UNIT};

/// Samples per cell edge that belong to the cell, the 33rd is the seam.
const CELL_STRIDE: usize = LAND_SAMPLES - 1;
const SAMPLE_COUNT: usize = LAND_SAMPLES * LAND_SAMPLES;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TerrainError {
    #[error("VHGT has {0} deltas, a cell has {SAMPLE_COUNT}")]
    DeltaCount(usize),
    #[error("cells per region must be a power of two from 2 to 64, got {0}")]
    CellsPerRegion(u32),
    #[error("cell heights have {0} samples, a cell has {SAMPLE_COUNT}")]
    SampleCount(usize),
}

/// Decode a VHGT block into absolute heights in Skyrim units, row-major
/// from the south-west corner. The first delta of each row is relative to
/// the first sample of the row below (row 0 to `offset`), the rest to the
/// sample to their west.
pub fn decode_vhgt(offset: f32, deltas: &[i8]) -> Result<Vec<f32>, TerrainError> {
    if deltas.len() != SAMPLE_COUNT {
        return Err(TerrainError::DeltaCount(deltas.len()));
    }
    let mut out = Vec::with_capacity(SAMPLE_COUNT);
    let mut row_start = offset;
    for row in deltas.as_chunks::<LAND_SAMPLES>().0 {
        row_start += row[0] as f32;
        let mut h = row_start;
        out.push(h * LAND_HEIGHT_UNITS);
        for &d in &row[1..] {
            h += d as f32;
            out.push(h * LAND_HEIGHT_UNITS);
        }
    }
    Ok(out)
}

/// One cell's decoded heights in Skyrim units.
#[derive(Debug, Clone, PartialEq)]
pub struct CellHeights {
    pub cell: CellCoord,
    /// 33 by 33, row-major from the south-west corner.
    pub units: Vec<f32>,
}

impl CellHeights {
    pub fn new(cell: CellCoord, units: Vec<f32>) -> Result<Self, TerrainError> {
        if units.len() != SAMPLE_COUNT {
            return Err(TerrainError::SampleCount(units.len()));
        }
        Ok(Self { cell, units })
    }

    /// A flat cell, for missing LAND records and tests.
    pub fn flat(cell: CellCoord, height_units: f32) -> Self {
        Self {
            cell,
            units: vec![height_units; SAMPLE_COUNT],
        }
    }

    fn sample(&self, col: usize, row: usize) -> f32 {
        self.units[row * LAND_SAMPLES + col]
    }
}

/// A Terrain3D region in the Godot region grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegionCoord {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionSpec {
    /// Cells along one edge of a region. 8 gives Terrain3D's default 256.
    pub cells_per_region: u32,
    /// Height in Skyrim units used where no cell was inserted. Tamriel's sea
    /// floor sits around -14000, so the default keeps holes under water.
    pub missing_height_units: f32,
}

impl Default for RegionSpec {
    fn default() -> Self {
        Self {
            cells_per_region: 8,
            missing_height_units: -14000.0,
        }
    }
}

impl RegionSpec {
    pub fn validate(&self) -> Result<(), TerrainError> {
        let c = self.cells_per_region;
        if !(2..=64).contains(&c) || !c.is_power_of_two() {
            return Err(TerrainError::CellsPerRegion(c));
        }
        Ok(())
    }

    /// Vertices along one region edge, Terrain3D's `region_size`.
    pub fn region_size(&self) -> u32 {
        self.cells_per_region * CELL_STRIDE as u32
    }

    /// Terrain3D's `vertex_spacing`, in metres.
    pub fn vertex_spacing_m(&self) -> f32 {
        CELL_METRES / CELL_STRIDE as f32
    }

    /// Region edge in metres.
    pub fn region_metres(&self) -> f32 {
        self.cells_per_region as f32 * CELL_METRES
    }

    /// The region a cell belongs to, in Skyrim cell-grid terms (north is +y).
    pub fn region_of(&self, cell: CellCoord) -> RegionCoord {
        let c = self.cells_per_region as i32;
        RegionCoord {
            x: cell.x.div_euclid(c),
            y: cell.y.div_euclid(c),
        }
    }

    /// Where Terrain3D places this region: its `region_location`, which is
    /// the floor of the Godot position over the region edge. The flip to
    /// -Z shifts the row index by one, see the module note.
    pub fn godot_region_location(&self, region: RegionCoord) -> (i32, i32) {
        (region.x, -(region.y + 1))
    }
}

/// A region's heights, ready to become a `FORMAT_RF` image.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionHeightmap {
    pub region: RegionCoord,
    pub godot_location: (i32, i32),
    pub size: u32,
    /// `size * size` metres, row-major, row 0 at the region's most negative
    /// Godot Z (its northern edge in Skyrim terms).
    pub heights_m: Vec<f32>,
    pub min_m: f32,
    pub max_m: f32,
    /// How many of the region's cells had real data.
    pub cells_present: u32,
}

impl RegionHeightmap {
    pub fn height_at(&self, col: u32, row: u32) -> f32 {
        self.heights_m[(row * self.size + col) as usize]
    }

    /// Little-endian `f32` bytes, the layout `Image.create_from_data` wants
    /// for `FORMAT_RF`.
    pub fn rf_bytes(&self) -> Vec<u8> {
        self.heights_m
            .iter()
            .flat_map(|h| h.to_le_bytes())
            .collect()
    }
}

#[derive(Debug)]
pub struct RegionBuilder {
    spec: RegionSpec,
    cells: HashMap<CellCoord, CellHeights>,
}

impl RegionBuilder {
    pub fn new(spec: RegionSpec) -> Result<Self, TerrainError> {
        spec.validate()?;
        Ok(Self {
            spec,
            cells: HashMap::new(),
        })
    }

    pub fn spec(&self) -> &RegionSpec {
        &self.spec
    }

    pub fn insert(&mut self, heights: CellHeights) {
        self.cells.insert(heights.cell, heights);
    }

    pub fn remove(&mut self, cell: CellCoord) -> Option<CellHeights> {
        self.cells.remove(&cell)
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    /// Regions that have at least one inserted cell.
    pub fn regions(&self) -> Vec<RegionCoord> {
        let mut v: Vec<RegionCoord> = self.cells.keys().map(|c| self.spec.region_of(*c)).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Assemble one region. Missing cells are filled, never an error, since
    /// Tamriel's edges have no LAND records at all.
    pub fn build(&self, region: RegionCoord) -> RegionHeightmap {
        let cpr = self.spec.cells_per_region as i32;
        let size = self.spec.region_size() as usize;
        let mut heights_m = vec![0.0f32; size * size];
        let mut min_m = f32::INFINITY;
        let mut max_m = f32::NEG_INFINITY;
        let mut present = 0u32;

        let filler = CellHeights::flat(CellCoord::new(0, 0), self.spec.missing_height_units);
        for local_y in 0..cpr {
            for local_x in 0..cpr {
                let cell = CellCoord::new(region.x * cpr + local_x, region.y * cpr + local_y);
                let data = match self.cells.get(&cell) {
                    Some(c) => {
                        present += 1;
                        c
                    }
                    None => &filler,
                };
                for sy in 0..CELL_STRIDE {
                    // LAND rows 1..=32 of this cell, row 32 being the seam it
                    // shares with its northern neighbour.
                    let land_row = sy + 1;
                    // Skyrim row index within the region counted from the
                    // south, then flipped so row 0 is the northern edge.
                    let k = local_y as usize * CELL_STRIDE + land_row;
                    let godot_row = size - k;
                    for sx in 0..CELL_STRIDE {
                        let col = local_x as usize * CELL_STRIDE + sx;
                        let h = data.sample(sx, land_row) * METRES_PER_UNIT;
                        heights_m[godot_row * size + col] = h;
                        min_m = min_m.min(h);
                        max_m = max_m.max(h);
                    }
                }
            }
        }
        RegionHeightmap {
            region,
            godot_location: self.spec.godot_region_location(region),
            size: size as u32,
            heights_m,
            min_m,
            max_m,
            cells_present: present,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cell whose sample (col, row) has height `base + col + 100 * row`
    /// in units, so any sample can be recognised.
    fn ramp(cell: CellCoord) -> CellHeights {
        let base = (cell.x * 1000 + cell.y * 100_000) as f32;
        let mut units = Vec::with_capacity(SAMPLE_COUNT);
        for row in 0..LAND_SAMPLES {
            for col in 0..LAND_SAMPLES {
                units.push(base + col as f32 + 100.0 * row as f32);
            }
        }
        CellHeights::new(cell, units).unwrap()
    }

    #[test]
    fn vhgt_deltas_chain_along_rows_and_down_the_first_column() {
        let mut deltas = vec![0i8; SAMPLE_COUNT];
        deltas[0] = 2; // row 0 starts at offset + 2
        deltas[1] = 1; // next sample east is one higher
        deltas[LAND_SAMPLES] = -1; // row 1 starts one below row 0's start
        let h = decode_vhgt(10.0, &deltas).unwrap();
        assert_eq!(h[0], 12.0 * 8.0);
        assert_eq!(h[1], 13.0 * 8.0);
        assert_eq!(h[2], 13.0 * 8.0);
        assert_eq!(h[LAND_SAMPLES], 11.0 * 8.0);
        assert_eq!(h.len(), SAMPLE_COUNT);
    }

    #[test]
    fn a_short_vhgt_is_refused() {
        assert_eq!(
            decode_vhgt(0.0, &[0; 10]).err(),
            Some(TerrainError::DeltaCount(10))
        );
        assert_eq!(
            CellHeights::new(CellCoord::new(0, 0), vec![0.0; 5]).err(),
            Some(TerrainError::SampleCount(5))
        );
    }

    #[test]
    fn spec_sizes_match_terrain3d_defaults() {
        let s = RegionSpec::default();
        assert_eq!(s.region_size(), 256);
        assert!((s.vertex_spacing_m() - 1.8288).abs() < 1e-3);
        assert!((s.region_metres() - 8.0 * CELL_METRES).abs() < 1e-3);
        assert_eq!(
            RegionSpec {
                cells_per_region: 3,
                ..RegionSpec::default()
            }
            .validate()
            .err(),
            Some(TerrainError::CellsPerRegion(3))
        );
    }

    #[test]
    fn cells_map_to_regions_with_euclidean_division() {
        let s = RegionSpec::default();
        assert_eq!(
            s.region_of(CellCoord::new(7, 0)),
            RegionCoord { x: 0, y: 0 }
        );
        assert_eq!(
            s.region_of(CellCoord::new(8, -1)),
            RegionCoord { x: 1, y: -1 }
        );
        assert_eq!(
            s.region_of(CellCoord::new(-1, -8)),
            RegionCoord { x: -1, y: -1 }
        );
        assert_eq!(s.godot_region_location(RegionCoord { x: 1, y: 0 }), (1, -1));
        assert_eq!(s.godot_region_location(RegionCoord { x: 0, y: -1 }), (0, 0));
    }

    #[test]
    fn a_region_is_stitched_from_its_cells_with_the_north_edge_on_row_zero() {
        let spec = RegionSpec {
            cells_per_region: 2,
            ..RegionSpec::default()
        };
        let mut b = RegionBuilder::new(spec).unwrap();
        for y in 0..2 {
            for x in 0..2 {
                b.insert(ramp(CellCoord::new(x, y)));
            }
        }
        let r = b.build(RegionCoord { x: 0, y: 0 });
        assert_eq!(r.size, 64);
        assert_eq!(r.cells_present, 4);
        assert_eq!(r.godot_location, (0, -1));

        // Godot row 63 (most positive Z, southern edge) is LAND row 1 of the
        // southern cells. Column 0 is cell (0, 0) sample col 0.
        let expect = |cell_x: i32, cell_y: i32, col: usize, row: usize| {
            (cell_x * 1000 + cell_y * 100_000) as f32 + col as f32 + 100.0 * row as f32
        };
        let m = METRES_PER_UNIT;
        assert!((r.height_at(0, 63) - expect(0, 0, 0, 1) * m).abs() < 1e-3);
        // Godot row 0 is LAND row 32 of the northern cells.
        assert!((r.height_at(0, 0) - expect(0, 1, 0, 32) * m).abs() < 1e-3);
        // Column 32 is the eastern cell's column 0.
        assert!((r.height_at(32, 63) - expect(1, 0, 0, 1) * m).abs() < 1e-3);
        // Row 31 is the southern cell's row 32 seam, which equals the
        // northern cell's row 0 only if the data agrees. Here the ramp
        // makes them differ, so check the row 32 of the southern cell is
        // what landed.
        assert!((r.height_at(5, 32) - expect(0, 0, 5, 32) * m).abs() < 1e-3);
        assert!((r.height_at(5, 31) - expect(0, 1, 5, 1) * m).abs() < 1e-3);
        assert!(r.min_m <= r.max_m);
        assert_eq!(r.rf_bytes().len(), 64 * 64 * 4);
    }

    #[test]
    fn a_missing_cell_is_filled_below_sea_level_and_counted() {
        let spec = RegionSpec {
            cells_per_region: 2,
            missing_height_units: -14000.0,
        };
        let mut b = RegionBuilder::new(spec).unwrap();
        b.insert(CellHeights::flat(CellCoord::new(0, 0), 0.0));
        let r = b.build(RegionCoord { x: 0, y: 0 });
        assert_eq!(r.cells_present, 1);
        assert_eq!(r.height_at(0, 63), 0.0);
        assert!((r.height_at(40, 10) + 14000.0 * METRES_PER_UNIT).abs() < 1e-3);
        assert_eq!(b.regions(), vec![RegionCoord { x: 0, y: 0 }]);
    }

    #[test]
    fn seams_between_cells_are_exact_when_the_data_agrees() {
        let spec = RegionSpec {
            cells_per_region: 2,
            ..RegionSpec::default()
        };
        let mut b = RegionBuilder::new(spec).unwrap();
        // A continuous world function sampled per cell: both cells sharing an
        // edge store the same values there.
        let world = |gx: i32, gy: i32| (gx * 3 + gy * 7) as f32;
        for cy in 0..2 {
            for cx in 0..2 {
                let mut units = Vec::with_capacity(SAMPLE_COUNT);
                for row in 0..LAND_SAMPLES as i32 {
                    for col in 0..LAND_SAMPLES as i32 {
                        units.push(world(cx * 32 + col, cy * 32 + row));
                    }
                }
                b.insert(CellHeights::new(CellCoord::new(cx, cy), units).unwrap());
            }
        }
        let r = b.build(RegionCoord { x: 0, y: 0 });
        for row in 0..64 {
            for col in 0..64 {
                let k = 64 - row as i32; // Skyrim sample row for this Godot row
                let want = world(col as i32, k) * METRES_PER_UNIT;
                assert!(
                    (r.height_at(col, row) - want).abs() < 1e-3,
                    "col {col} row {row}: {} vs {want}",
                    r.height_at(col, row)
                );
            }
        }
    }
}
