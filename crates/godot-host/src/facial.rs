// SPDX-License-Identifier: GPL-3.0-or-later

//! Apply the audio and race viseme mapping to a FaceGen head TRI.
//!
//! The audio crate decides which mouth shapes are active. The race record
//! supplies the named facial target weights, and the TRI supplies the vertex
//! deltas. Rebuilding each pose from the TRI's base vertices prevents one
//! sampled frame from accumulating into the next.

use soe_audio::viseme::{morph_weights, Shape, VisemeTrack};
use soe_esm::records::race::Race;
use soe_nif::tri::HeadTri;

/// A sampled face pose in the vertex order of its FaceGen TRI.
#[derive(Debug, Clone, PartialEq)]
pub struct FacialPose {
    /// Target weights in the order declared by the race's `PHTN` record.
    pub weights: Vec<(String, f32)>,
    /// Positions evaluated from the TRI base vertices.
    pub positions: Vec<[f32; 3]>,
}

/// Evaluate one viseme sample against a race's authored facial targets.
///
/// Targets declared by the race but absent from the TRI remain in `weights`
/// and do not change geometry. This lets race data keep its own meaning when
/// a particular head TRI only carries a subset of the targets.
///
/// # Errors
/// Returns a NIF error when a TRI morph weight is non-finite or its vertex
/// count does not match the TRI.
pub fn sample_facial_pose(
    track: &VisemeTrack,
    seconds: f64,
    blend: f64,
    race: &Race,
    tri: &HeadTri,
) -> Result<FacialPose, soe_nif::Error> {
    let shapes = track.shapes_at(seconds, blend);
    sample_shape_weights(&shapes, race, tri)
}

/// Evaluate weighted viseme shapes against the race and TRI.
///
/// This entry point is useful for native LIP tracks, which already carry
/// target weights and do not use Rhubarb cue sampling.
///
/// # Errors
/// Returns a NIF error when a TRI morph weight is non-finite or its vertex
/// count does not match the TRI.
pub fn sample_shape_weights(
    shapes: &[(Shape, f32)],
    race: &Race,
    tri: &HeadTri,
) -> Result<FacialPose, soe_nif::Error> {
    let weights = morph_weights(shapes, race);
    let mut positions = tri.vertices().to_vec();
    for (name, weight) in &weights {
        if tri.morphs().iter().any(|morph| morph.name() == *name) {
            positions = tri.apply_morph(&positions, name, *weight)?;
        }
    }
    Ok(FacialPose {
        weights: weights
            .into_iter()
            .map(|(name, weight)| (name.to_owned(), weight))
            .collect(),
        positions,
    })
}
