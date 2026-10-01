// SPDX-License-Identifier: GPL-3.0-or-later

//! ObjectReference translation follows the local CK `TranslateTo` and
//! `SplineTranslateTo` contracts for absolute targets, speed and events.

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use super::{Alloc, Handle, Pending, RefState, World};

use super::save::FormReader;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

/// The curve used by an in-flight reference translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TranslationPath {
    /// Move directly from the starting placement to the target.
    Linear,
    /// Follow a cubic curve whose endpoint tangents use this magnitude.
    Spline { tangent_magnitude: f32 },
}

/// Runtime state needed to continue a reference translation on later ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefTranslation {
    pub start_position: [f32; 3],
    pub target_position: [f32; 3],
    pub start_rotation: [f32; 3],
    pub target_rotation: [f32; 3],
    pub speed: f32,
    pub max_rotation_speed: f32,
    pub path: TranslationPath,
    pub elapsed: f32,
    pub duration: f32,
    pub almost_complete_fired: bool,
}

pub(crate) fn write_section(state: &RefState, writer: &mut Writer) {
    let Some(translation) = state.translation else {
        writer.bool(false);
        return;
    };
    writer.bool(true);
    for value in translation
        .start_position
        .into_iter()
        .chain(translation.target_position)
        .chain(translation.start_rotation)
        .chain(translation.target_rotation)
    {
        writer.f32(value);
    }
    writer.f32(translation.speed);
    writer.f32(translation.max_rotation_speed);
    match translation.path {
        TranslationPath::Linear => {
            writer.u8(0);
            writer.f32(0.0);
        }
        TranslationPath::Spline { tangent_magnitude } => {
            writer.u8(1);
            writer.f32(tangent_magnitude);
        }
    }
    writer.f32(translation.elapsed);
    writer.f32(translation.duration);
    writer.bool(translation.almost_complete_fired);
}

pub(crate) fn read_section(
    state: &mut RefState,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    if !reader.bool()? {
        state.translation = None;
        return Ok(());
    }
    let mut values = [0.0; 12];
    for value in &mut values {
        *value = reader.f32()?;
    }
    let speed = reader.f32()?;
    let max_rotation_speed = reader.f32()?;
    let path_code = reader.u8()?;
    let tangent_magnitude = reader.f32()?;
    let elapsed = reader.f32()?;
    let duration = reader.f32()?;
    let almost_complete_fired = reader.bool()?;
    let path = match path_code {
        0 if tangent_magnitude == 0.0 => TranslationPath::Linear,
        1 => TranslationPath::Spline { tangent_magnitude },
        _ => return Err(VmError::State("invalid HST-16 translation path".into())),
    };
    let valid = values.into_iter().all(f32::is_finite)
        && speed.is_finite()
        && speed > 0.0
        && max_rotation_speed.is_finite()
        && max_rotation_speed >= 0.0
        && tangent_magnitude.is_finite()
        && tangent_magnitude >= 0.0
        && elapsed.is_finite()
        && duration.is_finite()
        && elapsed >= 0.0
        && duration >= 0.0;
    if !valid {
        return Err(VmError::State("invalid HST-16 translation state".into()));
    }
    state.translation = Some(RefTranslation {
        start_position: [values[0], values[1], values[2]],
        target_position: [values[3], values[4], values[5]],
        start_rotation: [values[6], values[7], values[8]],
        target_rotation: [values[9], values[10], values[11]],
        speed,
        max_rotation_speed,
        path,
        elapsed,
        duration,
        almost_complete_fired,
    });
    Ok(())
}

impl World {
    /// Begin moving a reference toward absolute position and rotation targets.
    pub fn start_translation(
        &mut self,
        id: GlobalFormId,
        position: [f32; 3],
        rotation: [f32; 3],
        speed: f32,
        max_rotation_speed: f32,
        path: TranslationPath,
        alloc: Alloc<'_>,
    ) {
        let was_translating = self
            .reference(id)
            .is_some_and(|state| state.translation.take().is_some());
        if was_translating {
            self.translation_event(id, "OnTranslationFailed", alloc);
        }
        let Some(state) = self.reference(id) else {
            return;
        };
        let valid = position.into_iter().chain(rotation).all(f32::is_finite)
            && speed.is_finite()
            && speed > 0.0
            && max_rotation_speed.is_finite()
            && max_rotation_speed >= 0.0
            && match path {
                TranslationPath::Linear => true,
                TranslationPath::Spline { tangent_magnitude } => {
                    tangent_magnitude.is_finite() && tangent_magnitude >= 0.0
                }
            };
        if !valid || !state.enabled || state.deleted {
            state.translation = None;
            return;
        }
        let distance = match path {
            TranslationPath::Linear => length(sub(position, state.position)),
            TranslationPath::Spline { tangent_magnitude } => spline_length(
                state.position,
                position,
                state.rotation,
                rotation,
                tangent_magnitude,
            ),
        };
        let duration = distance / speed;
        if !duration.is_finite() {
            state.translation = None;
            return;
        }
        state.translation = Some(RefTranslation {
            start_position: state.position,
            target_position: position,
            start_rotation: state.rotation,
            target_rotation: rotation,
            speed,
            max_rotation_speed,
            path,
            elapsed: 0.0,
            duration,
            almost_complete_fired: false,
        });
        if duration == 0.0 && state.rotation == rotation {
            state.translation = None;
        }
    }

    /// Stop a reference translation and queue `OnTranslationFailed`.
    pub fn stop_translation(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        self.fail_translation(id, alloc)
    }

    /// Stop a translation at a collision boundary and queue `OnTranslationFailed`.
    pub fn block_translation(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        self.fail_translation(id, alloc)
    }

    fn fail_translation(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        let Some(state) = self.reference(id) else {
            return false;
        };
        if state.translation.take().is_none() {
            return false;
        }
        self.translation_event(id, "OnTranslationFailed", alloc);
        true
    }

    /// Advance translations in stable FormID order and queue their terminal events.
    pub fn tick_translations(&mut self, seconds: f64, alloc: Alloc<'_>) {
        if !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        let mut ids: Vec<_> = self.refs.keys().copied().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(state) = self.refs.get_mut(&id) else {
                continue;
            };
            let Some(mut translation) = state.translation else {
                continue;
            };
            if !state.enabled || state.deleted {
                state.translation = None;
                self.translation_event(id, "OnTranslationFailed", alloc);
                continue;
            }
            translation.elapsed = (translation.elapsed + seconds as f32)
                .min(translation.duration.max(rotation_duration(&translation)));
            let position_fraction = if translation.duration == 0.0 {
                1.0
            } else {
                (translation.elapsed / translation.duration).min(1.0)
            };
            state.position = match translation.path {
                TranslationPath::Linear => lerp3(
                    translation.start_position,
                    translation.target_position,
                    position_fraction,
                ),
                TranslationPath::Spline { tangent_magnitude } => spline_position(
                    translation.start_position,
                    translation.target_position,
                    translation.start_rotation,
                    translation.target_rotation,
                    spline_parameter_for_distance(
                        translation.start_position,
                        translation.target_position,
                        translation.start_rotation,
                        translation.target_rotation,
                        tangent_magnitude,
                        position_fraction,
                    ),
                    tangent_magnitude,
                ),
            };
            let rotation_fraction = if translation.max_rotation_speed == 0.0 {
                position_fraction
            } else {
                let angle = length(sub(translation.target_rotation, translation.start_rotation));
                if angle == 0.0 {
                    1.0
                } else {
                    (translation.elapsed * translation.max_rotation_speed.to_radians() / angle)
                        .min(position_fraction)
                }
            };
            state.rotation = lerp3(
                translation.start_rotation,
                translation.target_rotation,
                rotation_fraction,
            );
            let almost_complete = position_fraction >= 0.9 && !translation.almost_complete_fired;
            if almost_complete {
                translation.almost_complete_fired = true;
            }
            let arrived = position_fraction >= 1.0 && rotation_fraction >= 1.0;
            if arrived {
                if matches!(translation.path, TranslationPath::Spline { .. }) {
                    state.position = translation.target_position;
                }
                state.rotation = translation.target_rotation;
                state.translation = None;
            } else {
                state.translation = Some(translation);
            }
            if almost_complete {
                self.translation_event(id, "OnTranslationAlmostComplete", alloc);
            }
            if arrived {
                self.translation_event(id, "OnTranslationComplete", alloc);
            }
        }
    }

    fn translation_event(&mut self, id: GlobalFormId, name: &str, alloc: Alloc<'_>) {
        let object = self.object_of(Handle::Form(id), alloc);
        self.pending.push(Pending::Event {
            object,
            name: name.into(),
            args: Vec::<Value>::new(),
        });
    }
}

fn rotation_duration(translation: &RefTranslation) -> f32 {
    if translation.max_rotation_speed == 0.0 {
        return translation.duration;
    }
    let radians_per_second = translation.max_rotation_speed.to_radians();
    if radians_per_second <= 0.0 {
        return f32::INFINITY;
    }
    length(sub(translation.target_rotation, translation.start_rotation)) / radians_per_second
}

fn spline_position(
    start: [f32; 3],
    end: [f32; 3],
    start_rotation: [f32; 3],
    end_rotation: [f32; 3],
    t: f32,
    tangent: f32,
) -> [f32; 3] {
    let first = add(start, scale(forward(start_rotation), tangent));
    let second = sub(end, scale(forward(end_rotation), tangent));
    let inverse = 1.0 - t;
    add(
        add(
            scale(start, inverse * inverse * inverse),
            scale(first, 3.0 * inverse * inverse * t),
        ),
        add(scale(second, 3.0 * inverse * t * t), scale(end, t * t * t)),
    )
}

fn spline_length(
    start: [f32; 3],
    end: [f32; 3],
    start_rotation: [f32; 3],
    end_rotation: [f32; 3],
    tangent: f32,
) -> f32 {
    const SAMPLES: usize = 64;
    let mut previous = start;
    let mut length = 0.0;
    for sample in 1..=SAMPLES {
        let point = spline_position(
            start,
            end,
            start_rotation,
            end_rotation,
            sample as f32 / SAMPLES as f32,
            tangent,
        );
        length += length3(sub(point, previous));
        previous = point;
    }
    length
}

fn spline_parameter_for_distance(
    start: [f32; 3],
    end: [f32; 3],
    start_rotation: [f32; 3],
    end_rotation: [f32; 3],
    tangent: f32,
    fraction: f32,
) -> f32 {
    const SAMPLES: usize = 64;
    let mut points = [[0.0; 3]; SAMPLES + 1];
    points[0] = start;
    for (sample, point) in points.iter_mut().enumerate().skip(1) {
        *point = spline_position(
            start,
            end,
            start_rotation,
            end_rotation,
            sample as f32 / SAMPLES as f32,
            tangent,
        );
    }
    let total: f32 = points
        .windows(2)
        .map(|pair| length3(sub(pair[1], pair[0])))
        .sum();
    let wanted = total * fraction;
    let mut traversed = 0.0;
    for (segment, pair) in points.windows(2).enumerate() {
        let segment_length = length3(sub(pair[1], pair[0]));
        if traversed + segment_length >= wanted {
            let within = if segment_length == 0.0 {
                0.0
            } else {
                (wanted - traversed) / segment_length
            };
            return (segment as f32 + within) / SAMPLES as f32;
        }
        traversed += segment_length;
    }
    1.0
}

fn forward(rotation: [f32; 3]) -> [f32; 3] {
    let (pitch_sin, pitch_cos) = rotation[1].sin_cos();
    let (yaw_sin, yaw_cos) = rotation[2].sin_cos();
    [pitch_cos * yaw_cos, pitch_cos * yaw_sin, pitch_sin]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    add(a, scale(sub(b, a), t))
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f32; 3], amount: f32) -> [f32; 3] {
    [a[0] * amount, a[1] * amount, a[2] * amount]
}

fn length(a: [f32; 3]) -> f32 {
    length3(a)
}

fn length3(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hst_16_spline_position_snaps_to_its_endpoint() {
        assert_eq!(
            spline_position([0.0; 3], [10.0, 0.0, 0.0], [0.0; 3], [0.0; 3], 1.0, 8.0),
            [10.0, 0.0, 0.0]
        );
    }
}
