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
