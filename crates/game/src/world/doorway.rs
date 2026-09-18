// SPDX-License-Identifier: GPL-3.0-or-later

//! A load door from the moment it is activated to the moment the destination
//! attaches.
//!
//! Vanilla does both at once behind a loading screen. Splitting them needs
//! two things this module holds: somewhere to keep what the activation
//! decided while the door swings, and a test for when the player has actually
//! crossed.
//!
//! The crossing test is a signed distance to the door's plane with a band
//! around it, taken from `godotwind`'s `_update_walkthrough`
//! (`../engine-refs/godotwind/src/core/world/interior_pocket_manager.gd`),
//! which uses 0.3 m. That project triggers entry at the plane itself and only
//! applies the band on the way out, because its crossing changes a picture.
//! Ours queues `OnCellAttach` and `OnCellDetach`, so a player shuffling on
//! the threshold would spray events at any script listening. The band applies
//! in both directions here for that reason.

use soe_esm::records::Transform;
use soe_esm::GlobalFormId;

use super::{CellTransition, World};

/// Skyrim units in a metre. `crates/godot-host` states the same constant the
/// other way round as `METRES_PER_UNIT`, and this crate does not depend on
/// the host, so the value is repeated rather than shared.
pub const UNITS_PER_METRE: f32 = 70.0;

/// Half-width of the band around a door plane, 0.3 m in Skyrim units.
pub const THRESHOLD_BAND: f32 = 0.3 * UNITS_PER_METRE;

/// Which side of a door's plane a point is on. `Near` is the side the door
/// was activated from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Near,
    Far,
}

/// A door's plane, and which side the player was last known to be on.
#[derive(Debug, Clone, Copy)]
pub struct Threshold {
    point: [f32; 3],
    normal: [f32; 3],
    band: f32,
    side: Side,
}

impl Threshold {
    /// A plane through `point` whose positive side is `normal`. The normal is
    /// normalised here, so a caller may pass a door's facing unscaled.
    #[must_use]
    pub fn new(point: [f32; 3], normal: [f32; 3]) -> Self {
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let unit = if len > f32::EPSILON {
            [normal[0] / len, normal[1] / len, normal[2] / len]
        } else {
            [0.0, 1.0, 0.0]
        };
        Self {
            point,
            normal: unit,
            band: THRESHOLD_BAND,
            side: Side::Near,
        }
    }

    /// Override the band. Mostly for tests, which want a value they can
    /// reason about without converting metres.
    #[must_use]
    pub fn with_band(mut self, band: f32) -> Self {
        self.band = band;
        self
    }

    #[must_use]
    pub fn side(&self) -> Side {
        self.side
    }

    /// Positive on the near side, negative once through.
    #[must_use]
    pub fn signed_distance(&self, p: [f32; 3]) -> f32 {
        (p[0] - self.point[0]) * self.normal[0]
            + (p[1] - self.point[1]) * self.normal[1]
            + (p[2] - self.point[2]) * self.normal[2]
    }

    /// Feed the player's position. Returns the side newly arrived on, and
    /// `None` while the player is inside the band or has not changed sides.
    ///
    /// A position exactly on the plane changes nothing, which is what stops a
    /// player standing in a doorway from attaching and detaching a cell every
    /// frame.
    pub fn update(&mut self, p: [f32; 3]) -> Option<Side> {
        let d = self.signed_distance(p);
        let want = if d < -self.band {
            Side::Far
        } else if d > self.band {
            Side::Near
        } else {
            return None;
        };
        if want == self.side {
            return None;
        }
        self.side = want;
        Some(want)
    }
}

/// A load door the player has activated, holding what the activation decided
/// until the crossing commits it.
#[derive(Debug, Clone)]
pub struct Doorway {
    /// The door activated.
    pub door: GlobalFormId,
    /// The cell on the far side, `None` when the far door names none.
    pub to: Option<GlobalFormId>,
    /// Where the far door puts the player.
    pub landing: Transform,
    /// Whether the destination's 3D was absent when the door was activated,
    /// read before the prefetch. Carried rather than recomputed so that
    /// prefetching cannot change whether `OnCellLoad` fires.
    pub cold: bool,
    /// References the prefetch built. Zero means the destination was already
    /// resident and the crossing was always going to be instant.
    pub prefetched: usize,
    /// The plane the player has to cross for the destination to attach.
    pub threshold: Threshold,
}

impl World {
    /// Activating a load door: work out where it goes, note whether the
    /// destination is cold, and build it.
    ///
    /// Queues nothing and moves nobody. Everything a script can observe waits
    /// for [`World::commit_doorway`]. Returns `None` when the reference is not
    /// a load door, which is the caller's signal to fall back to the plain
    /// swinging-door path.
    pub fn arm_doorway(&mut self, door: GlobalFormId) -> Option<Doorway> {
        let (far_door, landing) = self.reference(door).and_then(|r| r.teleport)?;
        let to = self.reference(far_door).and_then(|d| d.cell);
        // Read coldness first. After the prefetch every answer is "resident".
        let cold = to.is_some_and(|c| !self.cell_is_resident(c));
        let prefetched = to.map_or(0, |c| self.prefetch_cell(c));
        let threshold = Threshold::new(landing.position, door_normal(landing.rotation));
        Some(Doorway {
            door,
            to,
            landing,
            cold,
            prefetched,
            threshold,
        })
    }

    /// The player crossed: attach the destination and detach what was
    /// resident, in one go.
    ///
    /// This is the whole of what a script sees, and after `arm_doorway` has
    /// run there is nothing left to build, so it costs a walk of the cell's
    /// reference list.
    pub fn commit_doorway(&mut self, way: &Doorway) -> CellTransition {
        let player = self.player();
        if let Some(r) = self.reference(player) {
            r.position = way.landing.position;
            r.rotation = way.landing.rotation;
            r.cell = way.to;
        }
        self.change_cell_with(way.to, way.cold)
    }
}

/// A door's facing from its rotation. The record stores Euler angles in
/// radians and a door's local forward is +Y, so only the Z rotation matters
/// for a plane that stands upright.
fn door_normal(rotation: [f32; 3]) -> [f32; 3] {
    let z = rotation[2];
    [-z.sin(), z.cos(), 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> Threshold {
        Threshold::new([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]).with_band(10.0)
    }

    #[test]
    fn a_normal_is_normalised_so_distance_is_in_world_units() {
        let t = Threshold::new([0.0, 0.0, 0.0], [0.0, 5.0, 0.0]);
        assert!((t.signed_distance([0.0, 3.0, 0.0]) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_degenerate_normal_does_not_produce_nan() {
        let t = Threshold::new([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        assert!(t.signed_distance([1.0, 2.0, 3.0]).is_finite());
    }

    #[test]
    fn crossing_the_band_changes_side_once() {
        let mut t = plane();
        assert_eq!(t.side(), Side::Near);
        assert_eq!(t.update([0.0, 50.0, 0.0]), None, "same side, no crossing");
        assert_eq!(t.update([0.0, -50.0, 0.0]), Some(Side::Far));
        assert_eq!(t.update([0.0, -60.0, 0.0]), None, "already far");
        assert_eq!(t.update([0.0, 50.0, 0.0]), Some(Side::Near));
    }

    #[test]
    fn standing_in_the_doorway_never_crosses() {
        let mut t = plane();
        let mut crossings = 0;
        for i in 0..200 {
            // Jitter across the plane itself, well inside the band.
            let y = if i % 2 == 0 { 0.5 } else { -0.5 };
            if t.update([0.0, y, 0.0]).is_some() {
                crossings += 1;
            }
        }
        assert_eq!(crossings, 0, "a player on the threshold sprayed events");
    }

    #[test]
    fn the_band_has_to_be_cleared_in_both_directions() {
        let mut t = plane();
        // Just short of the band on the far side does nothing.
        assert_eq!(t.update([0.0, -9.9, 0.0]), None);
        assert_eq!(t.update([0.0, -10.1, 0.0]), Some(Side::Far));
        // Coming back, just short of the near edge also does nothing.
        assert_eq!(t.update([0.0, 9.9, 0.0]), None);
        assert_eq!(t.update([0.0, 10.1, 0.0]), Some(Side::Near));
    }

    #[test]
    fn the_shipped_band_is_thirty_centimetres() {
        assert!((THRESHOLD_BAND - 21.0).abs() < 1e-4, "{THRESHOLD_BAND}");
    }

    #[test]
    fn a_door_facing_north_has_a_north_normal() {
        let n = door_normal([0.0, 0.0, 0.0]);
        assert!((n[0]).abs() < 1e-6);
        assert!((n[1] - 1.0).abs() < 1e-6);
        // A quarter turn puts it west.
        let n = door_normal([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        assert!((n[0] + 1.0).abs() < 1e-6, "{n:?}");
        assert!((n[1]).abs() < 1e-6, "{n:?}");
    }
}
