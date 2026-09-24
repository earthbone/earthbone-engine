// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim's coordinate system and units, and the conversion into Godot's.
//!
//! Skyrim is right-handed with Z up: X east, Y north. Godot is right-handed
//! with Y up and the camera looking down -Z, so north is -Z. The map
//! `(x, y, z) -> (x, z, -y)` has determinant +1, so it keeps handedness and
//! triangle winding as seen from outside a mesh.
//!
//! One Skyrim unit is 1.428875 cm (70 units to the metre, 21.33 to the
//! foot). An exterior cell is 4096 units square. LAND heights are stored in
//! eighths of a unit.

/// Metres per Skyrim unit.
pub const METRES_PER_UNIT: f32 = 0.0142875;

/// Exterior cell edge in Skyrim units.
pub const CELL_UNITS: f32 = 4096.0;

/// Exterior cell edge in metres.
pub const CELL_METRES: f32 = CELL_UNITS * METRES_PER_UNIT;

/// LAND VHGT samples per cell edge (33 samples, 32 intervals).
pub const LAND_SAMPLES: usize = 33;

/// Multiplier from a decoded VHGT value to Skyrim units.
pub const LAND_HEIGHT_UNITS: f32 = 8.0;

/// A position in Skyrim world space, in Skyrim units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkyrimPos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A position in Godot world space, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GodotPos {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl SkyrimPos {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_godot(self) -> GodotPos {
        GodotPos {
            x: self.x * METRES_PER_UNIT,
            y: self.z * METRES_PER_UNIT,
            z: -self.y * METRES_PER_UNIT,
        }
    }
}

impl GodotPos {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_skyrim(self) -> SkyrimPos {
        SkyrimPos {
            x: self.x / METRES_PER_UNIT,
            y: -self.z / METRES_PER_UNIT,
            z: self.y / METRES_PER_UNIT,
        }
    }
}

/// Convert a direction (normal, tangent) with the same axis map and no scale.
pub fn direction_to_godot(v: [f32; 3]) -> [f32; 3] {
    [v[0], v[2], -v[1]]
}

/// The inverse of [`direction_to_godot`], for patching a baked Godot-space
/// normal or tangent back into Skyrim space before applying a runtime delta.
pub fn direction_to_skyrim(v: [f32; 3]) -> [f32; 3] {
    [v[0], -v[2], v[1]]
}

/// A rotation, uniform scale and origin in Godot space, laid out the way
/// `Transform3D` wants it: `basis` is row major with the scale folded in,
/// `origin` is the translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GodotTransform {
    pub basis: [[f32; 3]; 3],
    pub origin: GodotPos,
}

/// Convert a Skyrim placement (a row major rotation, a translation in Skyrim
/// units, and a uniform scale) to the Godot equivalent, for `MultiMesh`
/// instance transforms. `mesh_data_from` bakes the same rotation into vertex
/// data by calling `Mat33::apply` on positions already read as Skyrim-space
/// vectors, so `rotation` here is expected in that same row major layout.
///
/// Positions convert through `SkyrimPos::to_godot`, a linear map with no
/// translation offset (metres-per-unit times the axis permutation
/// `direction_to_godot`), so it distributes over the placement's rotate,
/// scale and translate. Writing the axis map as a matrix `A` with rows
/// `(1,0,0)`, `(0,0,1)`, `(0,-1,0)`, the same algebra that carries a Skyrim
/// direction through `direction_to_godot` carries a Skyrim rotation matrix
/// `R` through `A * R * A^-1` (`A` is orthogonal, so `A^-1` is `A`'s
/// transpose). Multiplying that out row by row: Godot basis row 0 is Skyrim
/// row 0 with the axis map applied, basis row 1 is Skyrim row 2, and basis
/// row 2 is Skyrim row 1 negated, each of those three rows itself axis
/// mapped the same way `direction_to_godot` maps a single vector. The
/// translation converts with the ordinary position map.
pub fn placement_to_godot(
    rotation: [[f32; 3]; 3],
    translation: [f32; 3],
    scale: f32,
) -> GodotTransform {
    let mapped = |row: [f32; 3]| direction_to_godot(row);
    let basis = [
        mapped(rotation[0]).map(|v| v * scale),
        mapped(rotation[2]).map(|v| v * scale),
        mapped(rotation[1]).map(|v| -v * scale),
    ];
    let origin = SkyrimPos::new(translation[0], translation[1], translation[2]).to_godot();
    GodotTransform { basis, origin }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn north_becomes_negative_z_and_up_becomes_y() {
        let p = SkyrimPos::new(0.0, 70.0, 140.0).to_godot();
        assert!((p.x).abs() < 1e-6);
        assert!((p.y - 2.00025).abs() < 1e-4, "{}", p.y);
        assert!((p.z + 1.000125).abs() < 1e-4, "{}", p.z);
    }

    #[test]
    fn round_trip_is_identity() {
        let p = SkyrimPos::new(1234.5, -777.25, 12.0);
        let back = p.to_godot().to_skyrim();
        assert!((back.x - p.x).abs() < 1e-3);
        assert!((back.y - p.y).abs() < 1e-3);
        assert!((back.z - p.z).abs() < 1e-3);
    }

    #[test]
    fn a_cell_is_about_58_metres() {
        assert!((CELL_METRES - 58.52).abs() < 0.01, "{CELL_METRES}");
    }

    #[test]
    fn direction_round_trip_is_identity() {
        let v = [1.0, -2.5, 3.25];
        let back = direction_to_skyrim(direction_to_godot(v));
        assert!((back[0] - v[0]).abs() < 1e-6);
        assert!((back[1] - v[1]).abs() < 1e-6);
        assert!((back[2] - v[2]).abs() < 1e-6);
    }

    #[test]
    fn the_axis_map_keeps_handedness() {
        // Cross product of mapped east and mapped north must be mapped up.
        let e = direction_to_godot([1.0, 0.0, 0.0]);
        let n = direction_to_godot([0.0, 1.0, 0.0]);
        let u = direction_to_godot([0.0, 0.0, 1.0]);
        let cross = [
            e[1] * n[2] - e[2] * n[1],
            e[2] * n[0] - e[0] * n[2],
            e[0] * n[1] - e[1] * n[0],
        ];
        assert_eq!(cross, u);
    }
}
