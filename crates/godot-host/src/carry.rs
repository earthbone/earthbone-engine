// SPDX-License-Identifier: GPL-3.0-or-later

//! Held Activate and a damped carry spring. Sources: CK OnGrab/OnRelease,
//! nif.xml rigid-body mass, Skyrim.esm fZKeyMaxForce and Godot impulse semantics.
//! The spring uses the resident Jolt body, retaining collision and velocity.

use soe_esm::GlobalFormId;
use soe_physics::Vec3;

pub(crate) const HOLD_SECONDS: f64 = 0.35;

pub(crate) fn max_force(world: &soe_game::World) -> f32 {
    match world.game_setting("fZKeyMaxForce") {
        Some(soe_papyrus::Value::Float(value)) if value.is_finite() && value >= 0.0 => value,
        _ => 100.0,
    }
}

#[derive(Default)]
pub(crate) struct ActivateHold {
    pending: Option<(Option<GlobalFormId>, f64, bool)>,
}

#[derive(Debug, PartialEq)]
pub(crate) enum ActivateGesture {
    None,
    Tap(GlobalFormId),
    Grab(GlobalFormId),
}

impl ActivateHold {
    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }

    pub(crate) fn step(
        &mut self,
        pressed: bool,
        held: bool,
        delta: f64,
        target: Option<GlobalFormId>,
    ) -> ActivateGesture {
        if pressed {
            self.pending = Some((target, 0.0, false));
        }
        let Some((candidate, elapsed, consumed)) = &mut self.pending else {
            return ActivateGesture::None;
        };
        if !held {
            let tap = (!*consumed).then_some(*candidate).flatten();
            self.pending = None;
            return tap.map_or(ActivateGesture::None, ActivateGesture::Tap);
        }
        if delta.is_finite() && delta > 0.0 {
            *elapsed += delta;
        }
        if !*consumed && *elapsed >= HOLD_SECONDS {
            *consumed = true;
            if *candidate == target {
                return target.map_or(ActivateGesture::None, ActivateGesture::Grab);
            }
        }
        ActivateGesture::None
    }
}

const STIFFNESS: f32 = 200.0;
const DAMPING: f32 = 40.0;

#[cfg(test)]
fn spring_impulse(
    error: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    mass: f32,
    delta: f32,
    max_force: f32,
) -> Option<Vec3> {
    spring_impulse_with_inertia(error, velocity, gravity, mass, 1.0 / mass, delta, max_force)
}

fn spring_impulse_with_inertia(
    error: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    mass: f32,
    inverse_response_mass: f32,
    delta: f32,
    max_force: f32,
) -> Option<Vec3> {
    if ![error, velocity, gravity]
        .iter()
        .all(|v| v.x.is_finite() && v.y.is_finite() && v.z.is_finite())
        || !mass.is_finite()
        || mass <= 0.0
        || !inverse_response_mass.is_finite()
        || inverse_response_mass <= 0.0
        || !delta.is_finite()
        || delta <= 0.0
        || !max_force.is_finite()
        || max_force < 0.0
    {
        return None;
    }
    let delta = delta.min(1.0 / 30.0);
    // CONTEXT: Implicit spring and damping integration keeps light NIF bodies stable.
    let denominator = 1.0 + (DAMPING + STIFFNESS * delta) * delta * inverse_response_mass;
    let force = (error * STIFFNESS - velocity * (DAMPING + STIFFNESS * delta))
        * (1.0 / denominator)
        - gravity * mass;
    let length = force.length();
    if !length.is_finite() {
        return None;
    }
    let force = if length > max_force {
        force * (max_force / length)
    } else {
        force
    };
    let impulse = force * delta;
    (impulse.x.is_finite() && impulse.y.is_finite() && impulse.z.is_finite()).then_some(impulse)
}

#[cfg(test)]
fn grip_impulse(
    error: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    mass: f32,
    delta: f32,
    max_force: f32,
) -> Option<Vec3> {
    grip_impulse_with_inertia(error, velocity, gravity, mass, 1.0 / mass, delta, max_force)
}

pub(crate) fn grip_impulse_with_inertia(
    error: Vec3,
    velocity: Vec3,
    gravity: Vec3,
    mass: f32,
    inverse_response_mass: f32,
    delta: f32,
    max_force: f32,
) -> Option<Vec3> {
    let strain = (mass / 2.0).min(1000.0).powi(2);
    let sag_distance = 0.18 * strain / (1.0 + strain);
    let sag = gravity * (sag_distance / gravity.length().max(0.001));
    spring_impulse_with_inertia(
        error + sag,
        velocity,
        gravity,
        mass,
        inverse_response_mass,
        delta,
        max_force,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round4_carried_mass_produces_resting_sag_below_the_force_cap() {
        let simulate = |mass| {
            let delta = 1.0 / 120.0;
            let gravity = -Vec3::Z * 9.8;
            let mut position = Vec3::ZERO;
            let mut velocity = Vec3::ZERO;
            for _ in 0..1200 {
                let impulse =
                    grip_impulse(-position, velocity, gravity, mass, delta, 100.0).unwrap();
                velocity = velocity + impulse * (1.0 / mass) + gravity * delta;
                position = position + velocity * delta;
            }
            -position.z
        };
        let light = simulate(0.2);
        let heavy = simulate(5.0);
        eprintln!("Round 4 hold sag: 0.2 kg={light} m, 5 kg={heavy} m");
        assert!(light < 0.02, "cup stays close to the hand");
        assert!(
            heavy > 0.15 && heavy < 0.35,
            "heavier clutter sags without falling indefinitely"
        );
    }

    #[test]
    fn round4_bannered_mare_named_object_mass_and_weight_survey() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
        let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
        let (placed, _) = source.placed_cell_colliders(cell).unwrap();
        let game = source.localized_game().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        let mut masses = Vec::new();
        for placed in placed {
            let mut world = game.world_mut();
            let base = world.base_of(placed.reference).unwrap();
            let record = world.record(base).unwrap();
            let name = record.editor_id().unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            let Some(family) = [
                "tankard", "cup", "basket", "barrel", "sack", "chair", "bowl", "plate", "kettle",
            ]
            .into_iter()
            .find(|family| lower.contains(family) && !lower.contains("cupboard")) else {
                continue;
            };
            if !seen.insert((family, base)) {
                continue;
            }
            let weight = crate::item_card::ItemCard::read(&mut world, base)
                .ok()
                .map(|card| card.weight);
            eprintln!("Round 4 Bannered Mare {family}: {name}, NIF mass={} kg, static={}, inventory weight={weight:?}", placed.collider.mass, placed.collider.is_static());
            if !placed.collider.is_static() {
                masses.push(placed.collider.mass);
            }
        }
        assert!(
            seen.iter().any(|(family, _)| *family == "basket")
                && seen
                    .iter()
                    .any(|(family, _)| matches!(*family, "cup" | "tankard"))
        );
        assert!(masses.len() >= 2);
        let min = masses.iter().copied().fold(f32::INFINITY, f32::min);
        let max = masses.iter().copied().fold(0.0, f32::max);
        assert!(
            max > min * 2.0,
            "survey must cover distinct authored masses: {masses:?}"
        );
    }

    fn follow(mass: f32, max_force: f32, steps: usize) -> f32 {
        let delta = 1.0 / 120.0;
        let mut position = Vec3::ZERO;
        let mut velocity = Vec3::ZERO;
        for _ in 0..steps {
            let impulse = spring_impulse(
                Vec3::new(1.0, 0.0, 0.0) - position,
                velocity,
                Vec3::ZERO,
                mass,
                delta,
                max_force,
            )
            .unwrap();
            assert!(impulse.length() <= max_force * delta + 1e-6);
            velocity = velocity + impulse * (1.0 / mass);
            position = position + velocity * delta;
        }
        (Vec3::new(1.0, 0.0, 0.0) - position).length()
    }

    #[test]
    fn hst_51_light_body_follows_while_heavy_body_lags() {
        let light_error = follow(1.0, 100.0, 30);
        let heavy_error = follow(10.0, 100.0, 30);
        eprintln!("HST-51 after 0.25 s: 1 kg error={light_error} m, 10 kg error={heavy_error} m");
        assert!(
            light_error < 0.4,
            "light body follows promptly: {light_error}"
        );
        assert!(heavy_error > 0.6, "heavy body lags: {heavy_error}");
        assert!(heavy_error > light_error * 2.0);
    }

    #[test]
    fn hst_51_small_mass_stays_stable_and_force_includes_gravity() {
        assert!(follow(0.01, 100.0, 240) < 0.001);
        for mass in [1.0, 10.0, 150.0] {
            let impulse = spring_impulse(
                Vec3::new(1.0, 0.0, 0.0) * 100.0,
                Vec3::ZERO,
                -Vec3::Z * 10.0,
                mass,
                0.01,
                100.0,
            )
            .unwrap();
            assert!(
                (impulse.length() - 1.0).abs() < 1e-6,
                "same total force cap for {mass} kg"
            );
        }
        let impulse =
            spring_impulse(Vec3::ZERO, Vec3::ZERO, -Vec3::Z * 10.0, 2.0, 0.01, 100.0).unwrap();
        assert!(
            (impulse.z - 0.2).abs() < 1e-6,
            "stationary hold compensates gravity"
        );
    }

    #[test]
    fn hst_51_force_limit_reads_winning_gmst_and_live_script_override() {
        let setting = |value: f32| {
            soe_esm::Record::new(soe_esm::Tag::new(b"GMST"), 42)
                .zsub(b"EDID", "fZKeyMaxForce")
                .sub(b"DATA", value.to_le_bytes())
        };
        let mut base = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        base.push_top(setting(100.0));
        let mut patch = soe_esm::Plugin::new(soe_esm::FileHeader {
            masters: vec!["Skyrim.esm".into()],
            ..Default::default()
        });
        patch.push_top(setting(75.0));
        let mut order = soe_esm::LoadOrder::new();
        order.add("Skyrim.esm", base).unwrap();
        order.add("Grab.esp", patch).unwrap();
        let mut world = soe_game::World::new(order);
        assert_eq!(max_force(&world), 75.0);
        for value in [25.0, 0.0, -1.0, f32::NAN, f32::INFINITY] {
            world
                .settings
                .insert("fzkeymaxforce".into(), soe_papyrus::Value::Float(value));
            let limit = max_force(&world);
            let expected = if value.is_finite() && value >= 0.0 {
                value
            } else {
                100.0
            };
            assert_eq!(limit, expected);
            let impulse = spring_impulse(
                Vec3::new(1.0, 0.0, 0.0) * 100.0,
                Vec3::ZERO,
                Vec3::ZERO,
                2.0,
                0.01,
                limit,
            )
            .unwrap();
            assert!((impulse.length() - expected * 0.01).abs() < 1e-6);
        }
        assert_eq!(
            max_force(&soe_game::World::new(soe_esm::LoadOrder::new())),
            100.0
        );
        for limit in [-1.0, f32::NAN, f32::INFINITY] {
            assert!(spring_impulse(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 1.0, 0.01, limit).is_none());
        }
    }

    #[test]
    fn hst_51_installed_grab_settings_and_nif_masses() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            return;
        };
        let path = std::path::Path::new(&data);
        let plugin = soe_esm::read_plugin(path.join("Skyrim.esm"))
            .unwrap()
            .unwrap();
        let mut settings = std::collections::BTreeMap::new();
        for record in plugin.records_of(soe_esm::Tag::new(b"GMST")) {
            let Some(name) = record.editor_id().filter(|name| name.starts_with("fZKey")) else {
                continue;
            };
            let bytes: [u8; 4] = record
                .find(soe_esm::Tag::new(b"DATA"))
                .unwrap()
                .try_into()
                .unwrap();
            let value = f32::from_le_bytes(bytes);
            eprintln!("HST-51 {name}={value}");
            assert!(value.is_finite());
            settings.insert(name, value);
        }
        assert_eq!(settings.get("fZKeyMaxForce"), Some(&100.0));
        assert_eq!(settings.get("fZKeyMaxForceWeightHigh"), Some(&150.0));
        drop(plugin);
        let source = crate::data_source::DataSource::open(path).unwrap();
        let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
        let (colliders, _) = source.placed_cell_colliders(cell).unwrap();
        let mut masses: Vec<_> = colliders
            .iter()
            .filter(|placed| !placed.collider.is_static())
            .map(|placed| placed.collider.mass)
            .collect();
        masses.sort_by(f32::total_cmp);
        let light = *masses.first().unwrap();
        let heavy = *masses.last().unwrap();
        assert!(light.is_finite() && light > 0.0 && heavy.is_finite() && heavy > light);
        let light_error = follow(light, settings["fZKeyMaxForce"], 30);
        let heavy_error = follow(heavy, settings["fZKeyMaxForce"], 30);
        eprintln!("HST-51 {} dynamic NIF colliders: {light} kg error={light_error} m, {heavy} kg error={heavy_error} m", masses.len());
        assert!(
            heavy_error > light_error,
            "installed heavy body follows more slowly"
        );
    }

    const TARGET: GlobalFormId = GlobalFormId {
        file: 0,
        object: 42,
    };

    #[test]
    fn hst_22_tap_activates_once_on_release_including_between_steps() {
        let mut hold = ActivateHold::default();
        assert_eq!(
            hold.step(true, true, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.1, None),
            ActivateGesture::Tap(TARGET)
        );
        assert_eq!(hold.step(false, false, 0.1, None), ActivateGesture::None);
        assert_eq!(
            hold.step(true, false, 0.01, Some(TARGET)),
            ActivateGesture::Tap(TARGET)
        );
    }

    #[test]
    fn hst_22_hold_grabs_once_and_release_does_not_activate() {
        let mut hold = ActivateHold::default();
        assert_eq!(
            hold.step(true, true, HOLD_SECONDS - 0.01, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, true, 0.02, Some(TARGET)),
            ActivateGesture::Grab(TARGET)
        );
        assert_eq!(
            hold.step(false, true, 1.0, Some(TARGET)),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.01, Some(TARGET)),
            ActivateGesture::None
        );
    }

    #[test]
    fn hst_22_cancel_or_changed_aim_does_not_activate_or_grab() {
        let mut hold = ActivateHold::default();
        hold.step(true, true, 0.1, Some(TARGET));
        assert_eq!(
            hold.step(false, true, HOLD_SECONDS, None),
            ActivateGesture::None
        );
        assert_eq!(
            hold.step(false, false, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
        hold.step(true, true, 0.1, Some(TARGET));
        hold.cancel();
        assert_eq!(
            hold.step(false, false, 0.1, Some(TARGET)),
            ActivateGesture::None
        );
    }

    #[test]
    fn hst_22_tap_uses_fixed_container_without_grabbing() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
        plugin.push_top(soe_esm::Record::new(soe_esm::Tag::new(b"CONT"), 41));
        plugin.push_top(
            soe_esm::Record::new(soe_esm::tag::REFR, TARGET.object)
                .sub(b"NAME", 41_u32.to_le_bytes())
                .sub(b"DATA", [0; 24]),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("Skyrim.esm", plugin).unwrap();
        let mut game = soe_game::Game::new(order);
        let player = game.player();
        let mut hold = ActivateHold::default();
        hold.step(true, true, 0.1, Some(TARGET));
        let ActivateGesture::Tap(reference) = hold.step(false, false, 0.01, Some(TARGET)) else {
            panic!("short Activate must use the container");
        };
        assert!(game.activate_deferred(reference, player));
        assert!(game.world().reference_ref(reference).unwrap().opened);
        assert_eq!(game.world().grabbed_reference(), None);
    }

    #[test]
    fn hst_22_spring_pulls_damps_and_compensates_gravity() {
        let impulse = spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * 0.1,
            Vec3::new(1.0, 0.0, 0.0) * 0.2,
            -Vec3::Z * 10.0,
            2.0,
            0.01,
            160.0,
        )
        .unwrap();
        assert!((impulse.x - (0.2 - 0.2 * 0.42) / 1.21).abs() < 1e-6);
        assert!((impulse.z - 0.2).abs() < 1e-6);
        let braking = spring_impulse(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::ZERO,
            1.0,
            0.01,
            160.0,
        )
        .unwrap();
        assert!(braking.x < 0.0);
    }

    #[test]
    fn hst_22_spring_bounds_force_and_rejects_invalid_inputs() {
        let impulse = spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * 100.0,
            Vec3::ZERO,
            Vec3::ZERO,
            2.0,
            1.0,
            160.0,
        )
        .unwrap();
        assert!((impulse.length() - 160.0 / 30.0).abs() < 1e-5);
        for mass in [0.0, -1.0, f32::NAN] {
            assert!(
                spring_impulse(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, mass, 0.01, 160.0).is_none()
            );
        }
        assert!(spring_impulse(
            Vec3::new(1.0, 0.0, 0.0) * f32::NAN,
            Vec3::ZERO,
            Vec3::ZERO,
            1.0,
            0.01,
            160.0
        )
        .is_none());
        assert!(spring_impulse(
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::ZERO,
            1.0,
            f32::INFINITY,
            160.0
        )
        .is_none());
    }
}
