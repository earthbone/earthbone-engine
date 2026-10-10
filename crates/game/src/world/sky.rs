// SPDX-License-Identifier: GPL-3.0-or-later

//! Record-backed skies, following UESP WTHR/CLMT and xEdit weather helpers.
//! OpenMW mwrender/sky.cpp supplies the separation of sky colour, cloud layers,
//! celestial bodies and transition weights. Distances remain Skyrim units.

use soe_esm::{GlobalFormId, Record, Tag};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::VmError;

use super::save::{read_opt_form_id, write_opt_form_id, FormReader};
use super::{WeatherRequest, World};

/// One cloud texture with the current time colour, alpha and UV velocity.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudLayer {
    pub texture: String,
    pub color: [f32; 3],
    pub alpha: f32,
    pub speed: [f32; 2],
}

/// A derived sky. All source fields are read from winning forms.
#[derive(Debug, Clone, PartialEq)]
pub struct SkyFrame {
    pub visible: bool,
    /// Interior lighting when the cell does not request sky lighting.
    pub interior: Option<InteriorLighting>,
    pub weather: GlobalFormId,
    /// Sky upper, near fog, unused cloud, ambient, sunlight, sun, stars,
    /// sky lower, horizon and the remaining WTHR NAM0 entries.
    pub colors: [[f32; 3]; 17],
    /// Near, far, power and maximum fog opacity, in Skyrim units.
    pub fog: [f32; 4],
    pub clouds: Vec<CloudLayer>,
    /// Direction towards the sun, in Skyrim's Z-up coordinate system.
    pub sun_direction: [f32; 3],
    pub sun_texture: String,
    pub glare_texture: String,
    pub sun_glare: f32,
    pub night_model: String,
    pub night_alpha: f32,
    pub moons: [bool; 2],
    /// Eight phases, starting at full and proceeding through waning to waxing.
    pub moon_phase: u8,
    pub phase_days: u8,
    pub moon_directions: [[f32; 3]; 2],
    pub transition: f32,
}

/// Climate sun intervals and moon flags decoded from CLMT/TNAM.
#[derive(Debug, Clone, PartialEq)]
pub struct Climate {
    pub timing: [f32; 4],
    pub moons: [bool; 2],
    pub phase_days: u8,
    pub sun_texture: String,
    pub glare_texture: String,
    pub night_model: String,
    pub weathers: Vec<(GlobalFormId, u32, Option<GlobalFormId>)>,
}

/// Interior lighting after applying CELL/XCLL inheritance from LGTM/DATA.
#[derive(Debug, Clone, PartialEq)]
pub struct InteriorLighting {
    pub ambient: [f32; 3],
    pub ambient_directions: [[f32; 3]; 6],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_far_color: [f32; 3],
    pub fog: [f32; 4],
    pub rotation: [f32; 2],
    pub fade: f32,
    pub fog_clip: f32,
    pub light_fade: [f32; 2],
}

impl Climate {
    /// Select adjacent sunrise/day/sunset/night slots continuously across midnight.
    pub fn slots(&self, hour: f32) -> (usize, usize, f32) {
        let [rise_begin, rise_end, set_begin, set_end] = self.timing;
        let rise = rise_begin.midpoint(rise_end);
        let set = set_begin.midpoint(set_end);
        let h = hour.rem_euclid(24.0);
        let (a, b, begin, end, h) = if h < rise_begin {
            (3, 3, 0.0, rise_begin, h)
        } else if h < rise {
            (3, 0, rise_begin, rise, h)
        } else if h < rise_end {
            (0, 1, rise, rise_end, h)
        } else if h < set_begin {
            (1, 1, rise_end, set_begin, h)
        } else if h < set {
            (1, 2, set_begin, set, h)
        } else if h < set_end {
            (2, 3, set, set_end, h)
        } else {
            (3, 3, set_end, 24.0, h)
        };
        (
            a,
            b,
            ((h - begin) / (end - begin).max(f32::EPSILON)).clamp(0.0, 1.0),
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SkyState {
    selection: Option<GlobalFormId>,
    target: Option<GlobalFormId>,
    from: Vec<(GlobalFormId, f32)>,
    started: f64,
    duration: f64,
    handled: Option<WeatherRequest>,
}

fn invalid(message: &str) -> VmError {
    VmError::State(format!("RND-18 {message}"))
}

fn field<'a>(record: &'a Record, tag: [u8; 4], sizes: &[usize]) -> Result<&'a [u8], VmError> {
    record
        .find(Tag(tag))
        .filter(|b| sizes.contains(&b.len()))
        .ok_or_else(|| invalid(&format!("invalid {} {} size", record.tag, Tag(tag))))
}

fn floats(bytes: &[u8]) -> Result<Vec<f32>, VmError> {
    if !bytes.len().is_multiple_of(4) {
        return Err(invalid("invalid weather float size"));
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| {
            let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            v.is_finite()
                .then_some(v)
                .ok_or_else(|| invalid("nonfinite weather value"))
        })
        .collect()
}

fn path(record: &Record, tag: [u8; 4]) -> String {
    record.find(Tag(tag)).map_or_else(String::new, |b| {
        String::from_utf8_lossy(b.split(|v| *v == 0).next().unwrap_or_default()).replace('\\', "/")
    })
}

fn rgb(bytes: &[u8], from: usize, to: usize, blend: f32) -> [f32; 3] {
    std::array::from_fn(|i| {
        let a = f32::from(bytes[from * 4 + i]) / 255.0;
        let b = f32::from(bytes[to * 4 + i]) / 255.0;
        a + (b - a) * blend
    })
}

impl World {
    /// Read an interior's lighting without applying exterior weather to it.
    ///
    /// # Errors
    /// Rejects malformed lighting bodies, links and nonfinite numeric fields.
    pub fn interior_lighting(&self, cell: GlobalFormId) -> Result<InteriorLighting, VmError> {
        let record = self.sky_record(cell, *b"CELL")?;
        let local = record.find(Tag(*b"XCLL"));
        let local = local
            .map(soe_esm::records::cell::Lighting::decode)
            .transpose()
            .map_err(|e| invalid(&e.to_string()))?;
        let template = self
            .sky_link(cell, *b"LTMP")?
            .map(|id| self.sky_record(id, *b"LGTM"))
            .transpose()?;
        let template = template
            .map(|r| {
                let mut light = soe_esm::records::cell::Lighting::decode(field(
                    r,
                    *b"DATA",
                    &[64, 68, 72, 76, 80, 84, 88, 92],
                )?)
                .map_err(|e| invalid(&e.to_string()))?;
                if let Some(colors) = r.find(Tag(*b"DALC")) {
                    if ![24, 28, 32].contains(&colors.len()) {
                        return Err(invalid("invalid LGTM DALC size"));
                    }
                    light.ambient_colors = std::array::from_fn(|i| {
                        soe_esm::records::cell::LightingColor([
                            colors[i * 4],
                            colors[i * 4 + 1],
                            colors[i * 4 + 2],
                            0,
                        ])
                    });
                } else {
                    light.ambient_colors = [soe_esm::records::cell::LightingColor([0; 4]); 6];
                }
                Ok(light)
            })
            .transpose()?;
        let flags = local
            .as_ref()
            .and_then(|l| l.inheritance_flags)
            .unwrap_or_default();
        let own = local
            .as_ref()
            .or(template.as_ref())
            .ok_or_else(|| invalid("missing interior lighting"))?;
        let source = |flag| {
            if flags & flag != 0 {
                template.as_ref().unwrap_or(own)
            } else {
                own
            }
        };
        let color = |c: soe_esm::records::cell::LightingColor| {
            std::array::from_fn(|i| f32::from(c.0[i]) / 255.0)
        };
        let number = |v: f32| -> Result<f32, VmError> {
            v.is_finite()
                .then_some(v)
                .ok_or_else(|| invalid("nonfinite interior lighting"))
        };
        Ok(InteriorLighting {
            ambient: color(source(1).ambient_color),
            ambient_directions: source(1).ambient_colors.map(color),
            directional: color(source(2).directional_color),
            fog_color: color(source(4).fog_near_color),
            fog_far_color: color(source(4).fog_far_color.unwrap_or(source(4).fog_near_color)),
            fog: [
                number(source(8).fog_near)?,
                number(source(16).fog_far)?,
                number(source(256).fog_power)?,
                number(source(512).fog_max.unwrap_or(1.0))?,
            ],
            rotation: [
                source(32).directional_rotation_xy as f32,
                source(32).directional_rotation_z as f32,
            ],
            fade: number(source(64).directional_fade)?,
            fog_clip: number(source(128).fog_clip_distance)?,
            light_fade: [
                number(source(1024).light_fade_begin.unwrap_or_default())?,
                number(source(1024).light_fade_end.unwrap_or_default())?,
            ],
        })
    }

    fn sky_record(&self, id: GlobalFormId, tag: [u8; 4]) -> Result<&Record, VmError> {
        self.record(id)
            .filter(|r| r.tag == Tag(tag) && !r.is_deleted())
            .ok_or_else(|| invalid(&format!("missing {} {id}", Tag(tag))))
    }

    fn sky_link(&self, owner: GlobalFormId, tag: [u8; 4]) -> Result<Option<GlobalFormId>, VmError> {
        let Some(b) = self.record(owner).and_then(|r| r.find(Tag(tag))) else {
            return Ok(None);
        };
        let raw = u32::from_le_bytes(b.try_into().map_err(|_| invalid("invalid sky form link"))?);
        Ok(self.resolve_from(owner, raw))
    }

    fn sky_cell_flags(&self, cell: GlobalFormId) -> Result<u16, VmError> {
        let bytes = field(self.sky_record(cell, *b"CELL")?, *b"DATA", &[1, 2])?;
        Ok(u16::from(bytes[0]) | (u16::from(*bytes.get(1).unwrap_or(&0)) << 8))
    }

    fn sky_weather_list(
        &self,
        owner: GlobalFormId,
        list: &[u8],
    ) -> Result<Vec<(GlobalFormId, u32, Option<GlobalFormId>)>, VmError> {
        if !list.len().is_multiple_of(12) {
            return Err(invalid("invalid weather list size"));
        }
        let mut weathers = Vec::new();
        for entry in list.as_chunks::<12>().0 {
            let raw = |offset| {
                u32::from_le_bytes([
                    entry[offset],
                    entry[offset + 1],
                    entry[offset + 2],
                    entry[offset + 3],
                ])
            };
            if let Some(weather) = self.resolve_from(owner, raw(0)) {
                self.sky_record(weather, *b"WTHR")?;
                let global = self.resolve_from(owner, raw(8));
                if let Some(global) = global {
                    self.sky_record(global, *b"GLOB")?;
                }
                weathers.push((weather, raw(4), global));
            }
        }
        Ok(weathers)
    }

    fn sky_region_weathers(
        &self,
        region: GlobalFormId,
    ) -> Result<Vec<(GlobalFormId, u32, Option<GlobalFormId>)>, VmError> {
        let r = self.sky_record(region, *b"REGN")?;
        let mut weather_header = false;
        for sub in &r.subrecords {
            if sub.tag == Tag(*b"RDAT") {
                if sub.data.len() != 8 {
                    return Err(invalid("invalid REGN RDAT size"));
                }
                weather_header = sub.data[..4] == 3_u32.to_le_bytes();
            } else if sub.tag == Tag(*b"RDWT") {
                if !weather_header {
                    return Err(invalid("REGN RDWT outside weather entry"));
                }
                return self.sky_weather_list(region, &sub.data);
            }
        }
        Ok(Vec::new())
    }

    /// Read the winning climate's timing, celestial textures and weighted weather list.
    ///
    /// # Errors
    /// Rejects missing, deleted and malformed climate records and invalid links.
    pub fn sky_climate(&self, id: GlobalFormId) -> Result<Climate, VmError> {
        let r = self.sky_record(id, *b"CLMT")?;
        let t = field(r, *b"TNAM", &[6])?;
        let timing = std::array::from_fn(|i| f32::from(t[i]) / 6.0);
        if timing.windows(2).any(|v| v[0] > v[1]) || timing[3] > 24.0 {
            return Err(invalid("invalid climate sun intervals"));
        }
        let list = r.find(Tag(*b"WLST")).unwrap_or_default();
        let weathers = self.sky_weather_list(id, list)?;
        Ok(Climate {
            timing,
            moons: [t[5] & 0x40 != 0, t[5] & 0x80 != 0],
            phase_days: (t[5] & 0x3f).max(1),
            sun_texture: path(r, *b"FNAM"),
            glare_texture: path(r, *b"GNAM"),
            night_model: path(r, *b"MODL"),
            weathers,
        })
    }

    /// Resolve a cell's owning world or XCCM region world to its inherited climate.
    ///
    /// # Errors
    /// Rejects malformed links, missing records and parent cycles.
    pub fn sky_climate_for_cell(
        &self,
        cell: GlobalFormId,
    ) -> Result<Option<GlobalFormId>, VmError> {
        let flags = self.sky_cell_flags(cell)?;
        if flags & 1 != 0 && flags & 0x180 == 0 {
            return Ok(None);
        }
        let index = self.order.build_cell_index();
        // CONTEXT: The frame calls this every tick, so the persistent-cell
        // owner comes from the index and not from a walk over every winner.
        let mut world = index.grid_of(cell).map(|g| g.world).or_else(|| {
            index.persistent_cell_world(cell).filter(|id| {
                self.order
                    .winner(*id)
                    .is_some_and(|r| r.tag == Tag(*b"WRLD"))
            })
        });
        if let Some(region) = self.sky_link(cell, *b"XCCM")? {
            self.sky_record(region, *b"REGN")?;
            world = self.sky_link(region, *b"WNAM")?.or(world);
        }
        let mut seen = std::collections::BTreeSet::new();
        while let Some(id) = world {
            if !seen.insert(id) {
                return Err(invalid("climate parent cycle"));
            }
            let r = self.sky_record(id, *b"WRLD")?;
            if r.find(Tag(*b"PNAM")).is_some_and(|b| b.len() != 2) {
                return Err(invalid("invalid WRLD PNAM size"));
            }
            let inherit = r
                .find(Tag(*b"PNAM"))
                .is_some_and(|b| b.first().is_some_and(|v| v & 0x10 != 0));
            if !inherit {
                return self.sky_link(id, *b"CNAM");
            }
            world = self.sky_link(id, *b"WNAM")?;
        }
        Ok(None)
    }

    /// Decode and sample one WTHR at the climate's current time and elapsed days.
    ///
    /// # Errors
    /// Rejects malformed colour, cloud, fog and transition fields.
    pub fn sample_sky(
        &self,
        weather: GlobalFormId,
        climate: &Climate,
        hour: f32,
        days: f64,
    ) -> Result<SkyFrame, VmError> {
        if !hour.is_finite() || !days.is_finite() {
            return Err(invalid("nonfinite sky time"));
        }
        let r = self.sky_record(weather, *b"WTHR")?;
        let colors = field(r, *b"NAM0", &[208, 224, 272])?;
        let data = field(r, *b"DATA", &[19])?;
        let (a, b, t) = climate.slots(hour);
        let mut sampled = [[0.0; 3]; 17];
        for (i, value) in sampled.iter_mut().enumerate() {
            if let Some(bytes) = colors.get(i * 16..i * 16 + 16) {
                *value = rgb(bytes, a, b, t);
            } else if i >= 14 {
                *value = [1.0; 3];
            }
        }
        let fog = floats(field(r, *b"FNAM", &[32])?)?;
        if fog.iter().any(|v| *v < 0.0) {
            return Err(invalid("negative fog value"));
        }
        let [rise_begin, rise_end, set_begin, set_end] = climate.timing;
        let h = hour.rem_euclid(24.0);
        let daylight = if h < rise_begin || h >= set_end {
            0.0
        } else if h < rise_end {
            (h - rise_begin) / (rise_end - rise_begin).max(f32::EPSILON)
        } else if h < set_begin {
            1.0
        } else {
            1.0 - (h - set_begin) / (set_end - set_begin).max(f32::EPSILON)
        };
        let fog = [
            fog[2] + (fog[0] - fog[2]) * daylight,
            fog[3] + (fog[1] - fog[3]) * daylight,
            fog[5] + (fog[4] - fog[5]) * daylight,
            fog[7] + (fog[6] - fog[7]) * daylight,
        ];
        let speed = |tag: [u8; 4]| -> Result<Vec<f32>, VmError> {
            if let Some(bytes) = r.find(Tag(tag)) {
                if bytes.len() != 32 {
                    return Err(invalid("invalid cloud speed size"));
                }
                Ok(bytes
                    .iter()
                    .map(|v| (f32::from(*v) - 127.0) / 1270.0)
                    .collect())
            } else {
                Ok(vec![0.0; 32])
            }
        };
        let xs = speed(*b"QNAM")?;
        let ys = speed(*b"RNAM")?;
        let cloud_colors = r.find(Tag(*b"PNAM"));
        let alphas = r.find(Tag(*b"JNAM")).map(floats).transpose()?;
        if cloud_colors.is_some_and(|v| v.len() != 512)
            || alphas.as_ref().is_some_and(|v| v.len() != 128)
        {
            return Err(invalid("invalid cloud colour or alpha size"));
        }
        let disabled = r
            .find(Tag(*b"NAM1"))
            .map(|bytes| {
                Ok::<_, VmError>(u32::from_le_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| invalid("invalid disabled layers"))?,
                ))
            })
            .transpose()?
            .unwrap_or_default();
        let mut clouds = Vec::new();
        let layers = r
            .find(Tag(*b"LNAM"))
            .map(|b| {
                let bytes: [u8; 4] = b
                    .try_into()
                    .map_err(|_| invalid("invalid cloud layer count size"))?;
                let count = u32::from_le_bytes(bytes);
                if count > 32 {
                    return Err(invalid("cloud layer count exceeds 32"));
                }
                Ok(count as usize)
            })
            .transpose()?
            .unwrap_or(32);
        for i in 0..layers {
            if disabled & (1 << i) != 0 {
                continue;
            }
            let mut texture = path(r, [b'0' + i as u8, b'0', b'T', b'X']);
            if texture.is_empty() && i < 4 {
                texture = path(r, [*b"DNAM", *b"CNAM", *b"ANAM", *b"BNAM"][i]);
            }
            if texture.is_empty() {
                continue;
            }
            let alpha = alphas
                .as_ref()
                .map_or(1.0, |v| v[i * 4 + a] + (v[i * 4 + b] - v[i * 4 + a]) * t);
            clouds.push(CloudLayer {
                texture,
                color: cloud_colors.map_or([1.0; 3], |v| rgb(&v[i * 16..i * 16 + 16], a, b, t)),
                alpha: alpha.clamp(0.0, 1.0),
                speed: [xs[i], ys[i]],
            });
        }
        let [rise_begin, rise_end, set_begin, set_end] = climate.timing;
        let rise = rise_begin.midpoint(rise_end);
        let set = set_begin.midpoint(set_end);
        let h = hour.rem_euclid(24.0);
        let angle = if h >= rise && h <= set {
            (h - rise) / (set - rise).max(0.01) * std::f32::consts::PI
        } else {
            (h - set).rem_euclid(24.0) / (24.0 - (set - rise)).max(0.01) * std::f32::consts::PI
                + std::f32::consts::PI
        };
        let orbit = |hour: f32| {
            let angle = (hour - 6.0) * std::f32::consts::PI / 12.0;
            [angle.cos(), 0.0, angle.sin()]
        };
        Ok(SkyFrame {
            visible: true,
            interior: None,
            weather,
            colors: sampled,
            fog,
            clouds,
            sun_direction: [angle.cos(), 0.0, angle.sin()],
            sun_texture: climate.sun_texture.clone(),
            glare_texture: climate.glare_texture.clone(),
            sun_glare: f32::from(data[4]) / 255.0,
            night_model: climate.night_model.clone(),
            night_alpha: 1.0 - daylight,
            moons: climate.moons,
            moon_phase: ((days.max(0.0) / f64::from(climate.phase_days)).floor() as u64 % 8) as u8,
            phase_days: climate.phase_days,
            moon_directions: [
                orbit(h + 12.0),
                orbit(h + 12.0 + (days.floor().rem_euclid(5.0) as f32) * 2.4),
            ],
            transition: 1.0,
        })
    }

    fn sky_weights(&self) -> Vec<(GlobalFormId, f32)> {
        let state = &self.weather.sky;
        let blend = if state.duration == 0.0 {
            1.0
        } else {
            ((self.clock.game_hours() - state.started) / state.duration).clamp(0.0, 1.0) as f32
        };
        let mut weights = std::collections::BTreeMap::new();
        for (id, weight) in &state.from {
            *weights.entry(*id).or_insert(0.0) += weight * (1.0 - blend);
        }
        if let Some(id) = state.target {
            *weights.entry(id).or_insert(0.0) += blend;
        }
        weights.into_iter().filter(|(_, w)| *w > 0.0).collect()
    }

    /// Current WTHR identity selected by the shared sky, including a transition target.
    /// CommonLibSSE-NG Sky distinguishes this from its outgoing lastWeather.
    #[must_use]
    pub fn current_sky_weather(&self) -> Option<GlobalFormId> {
        self.weather.sky.target
    }

    /// Select climate or explicit region weather and blend its sky presentation.
    /// Interior show-sky and sky-lighting flags are independent.
    ///
    /// # Errors
    /// Rejects malformed source records before replacing the active transition.
    pub fn sky_frame(&mut self, cell: GlobalFormId) -> Result<Option<SkyFrame>, VmError> {
        let prior = self.weather.clone();
        let result = self.sky_frame_inner(cell);
        if result.is_err() {
            self.weather = prior;
        }
        result
    }

    fn sky_frame_inner(&mut self, cell: GlobalFormId) -> Result<Option<SkyFrame>, VmError> {
        let Some(climate_id) = self.sky_climate_for_cell(cell)? else {
            return Ok(None);
        };
        let climate = self.sky_climate(climate_id)?;
        let region = self.sky_link(cell, *b"XCCM")?;
        let region_weathers = region.map(|id| self.sky_region_weathers(id)).transpose()?;
        let (selection, weathers) =
            if let Some(list) = region_weathers.as_ref().filter(|l| !l.is_empty()) {
                (region, list)
            } else {
                (Some(climate_id), &climate.weathers)
            };
        let request = self.weather.request;
        let state = &self.weather.sky;
        let requested = request.filter(|r| state.handled != Some(*r));
        let changed = state.selection != selection;
        let target = if let Some(request) = requested {
            Some(request.weather)
        } else if state.target.is_none() || changed && !request.is_some_and(|r| r.override_weather)
        {
            let choices: Vec<_> = weathers
                .iter()
                .filter_map(|(id, chance, global)| {
                    let weight = global
                        .and_then(|g| {
                            self.globals.get(&g).copied().or_else(|| {
                                self.record(g)?
                                    .find(Tag(*b"FLTV"))
                                    .filter(|b| b.len() == 4)
                                    .map(super::le_f32)
                            })
                        })
                        .map_or(*chance, |v| v.max(0.0) as u32);
                    (weight > 0).then_some((*id, weight))
                })
                .collect();
            let total: u64 = choices.iter().map(|(_, w)| u64::from(*w)).sum();
            if total == 0 {
                return Err(invalid("climate has no eligible weather"));
            }
            // CONTEXT: Cell identity makes initial selection repeatable without consuming gameplay RNG.
            let mut draw = (u64::from(cell.object).wrapping_mul(0x9e37_79b9)) % total;
            choices.into_iter().find_map(|(id, w)| {
                if draw < u64::from(w) {
                    Some(id)
                } else {
                    draw -= u64::from(w);
                    None
                }
            })
        } else {
            state.target
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let hour = self.calendar().hour as f32;
        let frame = self.sample_sky(target, &climate, hour, self.clock.game_days)?;
        if self.weather.sky.target != Some(target) {
            let from = self.sky_weights();
            let data = field(self.sky_record(target, *b"WTHR")?, *b"DATA", &[19])?;
            let duration = f64::from(data[3]) / 255.0 * 0.25;
            let duration = if requested.is_some_and(|r| r.accelerate) {
                duration.min(0.25 / 255.0)
            } else {
                duration
            };
            self.weather.sky.from = from;
            self.weather.sky.started = self.clock.game_hours();
            self.weather.sky.duration = if self.weather.sky.from.is_empty() {
                0.0
            } else {
                duration
            };
            self.weather.sky.target = Some(target);
        }
        self.weather.sky.selection = selection;
        self.weather.sky.handled = request;
        let weights = self.sky_weights();
        let mut output = frame;
        output.colors = [[0.0; 3]; 17];
        output.fog = [0.0; 4];
        output.clouds.clear();
        output.sun_glare = 0.0;
        for (id, weight) in weights {
            let sampled = self.sample_sky(id, &climate, hour, self.clock.game_days)?;
            for (color, source) in output.colors.iter_mut().zip(sampled.colors) {
                for (v, s) in color.iter_mut().zip(source) {
                    *v += s * weight;
                }
            }
            for (v, s) in output.fog.iter_mut().zip(sampled.fog) {
                *v += s * weight;
            }
            output.sun_glare += sampled.sun_glare * weight;
            output
                .clouds
                .extend(sampled.clouds.into_iter().map(|mut cloud| {
                    cloud.alpha *= weight;
                    cloud
                }));
        }
        output.transition = if self.weather.sky.duration == 0.0 {
            1.0
        } else {
            ((self.clock.game_hours() - self.weather.sky.started) / self.weather.sky.duration)
                .clamp(0.0, 1.0) as f32
        };
        let flags = self.sky_cell_flags(cell)?;
        if flags & 1 != 0 {
            output.visible = flags & 0x80 != 0;
            if flags & 0x100 == 0 {
                output.interior = Some(self.interior_lighting(cell)?);
            }
        }
        Ok(Some(output))
    }
}

pub(super) fn write_sky(world: &World, w: &mut Writer, _pool: &mut ArrayPool) {
    if world.weather.sky.target.is_none() && world.weather.request.is_none() {
        return;
    }
    let state = &world.weather.sky;
    for request in [world.weather.request, state.handled] {
        write_opt_form_id(w, request.map(|r| r.weather));
        if let Some(r) = request {
            w.bool(r.override_weather);
            w.bool(r.accelerate);
        }
    }
    write_opt_form_id(w, state.selection);
    write_opt_form_id(w, state.target);
    w.f64(state.started);
    w.f64(state.duration);
    w.len(state.from.len());
    for (id, weight) in &state.from {
        super::save::write_form_id(w, *id);
        w.f32(*weight);
    }
}

pub(super) fn read_sky(
    world: &mut World,
    r: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let mut request = || -> Result<Option<WeatherRequest>, VmError> {
        let Some(weather) = read_opt_form_id(r)? else {
            return Ok(None);
        };
        world.sky_record(weather, *b"WTHR")?;
        Ok(Some(WeatherRequest {
            weather,
            override_weather: r.bool()?,
            accelerate: r.bool()?,
        }))
    };
    let selected = request()?;
    let handled = request()?;
    let selection = read_opt_form_id(r)?;
    let target = read_opt_form_id(r)?;
    if let Some(id) = selection {
        if !world
            .record(id)
            .is_some_and(|r| !r.is_deleted() && [Tag(*b"CLMT"), Tag(*b"REGN")].contains(&r.tag))
        {
            return Err(invalid("invalid saved weather selection"));
        }
    }
    if let Some(id) = target {
        world.sky_record(id, *b"WTHR")?;
    }
    let started = r.f64()?;
    let duration = r.f64()?;
    let count = r.len()?;
    if !started.is_finite() || !duration.is_finite() || duration < 0.0 || count > r.remaining() / 10
    {
        return Err(invalid("invalid saved sky transition"));
    }
    let mut from = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..count {
        let id = super::save::read_form_id(r)?;
        let weight = r.f32()?;
        world.sky_record(id, *b"WTHR")?;
        if !weight.is_finite() || !(0.0..=1.0).contains(&weight) || !seen.insert(id) {
            return Err(invalid("invalid saved sky weight"));
        }
        from.push((id, weight));
    }
    if !from.is_empty() && (from.iter().map(|(_, w)| w).sum::<f32>() - 1.0).abs() > 0.001 {
        return Err(invalid("invalid saved sky weight sum"));
    }
    world.weather.request = selected;
    world.weather.sky = SkyState {
        selection,
        target,
        from,
        started,
        duration,
        handled,
    };
    Ok(())
}
