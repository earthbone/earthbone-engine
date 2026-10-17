// SPDX-License-Identifier: GPL-3.0-or-later

//! Streamed NIF volume emitters use Godot particle RIDs. The source timeline
//! submits births and the native particle shader advances their age and motion.

use super::*;
use crate::data_source::LoadedParticle;
use soe_nif::particle::{RenderEmitter, Timeline, Volume};

pub(super) struct ResidentParticle {
    pub cell: GlobalFormId,
    pub(super) reference: GlobalFormId,
    source: LoadedParticle,
    timeline: Timeline,
    elapsed: f32,
    submitted: u64,
    rid: Rid,
    particles: Rid,
    material: Gd<ShaderMaterial>,
    _process: Gd<ShaderMaterial>,
    _quad: Gd<QuadMesh>,
    placement: soe_nif::types::Transform,
    enabled: bool,
}

impl Drop for ResidentParticle {
    fn drop(&mut self) {
        let mut server = RenderingServer::singleton();
        server.free_rid(self.rid);
        server.free_rid(self.particles);
    }
}

/// Far range in game units that BSPSysLODModifier distance fractions scale.
/// The fractions carry no unit in nif.xml, one exterior cell width is the
/// working assumption until a play capture says otherwise.
const LOD_RANGE_UNITS: f32 = 4096.0;

/// Emit scale and size scale of a LOD modifier at a viewer distance in game units.
pub(super) fn lod_scales(lod: Option<[f32; 4]>, distance_units: f32) -> (f32, f32) {
    let Some([begin, end, emit, size]) = lod else {
        return (1.0, 1.0);
    };
    let begin = begin * LOD_RANGE_UNITS;
    let end = end * LOD_RANGE_UNITS;
    let t = if end > begin {
        ((distance_units - begin) / (end - begin)).clamp(0.0, 1.0)
    } else if distance_units >= end {
        1.0
    } else {
        0.0
    };
    (1.0 + (emit - 1.0) * t, 1.0 + (size - 1.0) * t)
}

fn random(index: u64, channel: u64) -> f32 {
    let mut x = index
        .wrapping_mul(6364136223846793005)
        .wrapping_add(channel.wrapping_mul(1442695040888963407));
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    ((x ^ (x >> 33)) >> 40) as f32 / 16777216.0
}

fn spawn(source: &RenderEmitter, index: u64) -> ([f32; 3], [f32; 3], f32) {
    let theta = random(index, 1) * std::f32::consts::TAU;
    let (sin, cos) = theta.sin_cos();
    let position = match source.volume {
        Volume::Cylinder { radius, height } => {
            let r = radius * random(index, 2).sqrt();
            [r * cos, r * sin, (random(index, 3) - 0.5) * height]
        }
        Volume::Sphere(radius) => {
            let z = 2.0 * random(index, 3) - 1.0;
            let r = radius * random(index, 2).cbrt();
            let xy = (1.0 - z * z).sqrt();
            [r * xy * cos, r * xy * sin, r * z]
        }
        Volume::Box(size) => std::array::from_fn(|i| size[i] * (random(index, i as u64 + 1) - 0.5)),
    };
    let [speed, variation, declination, declination_variation, planar, planar_variation] =
        source.velocity;
    let speed = speed + variation * random(index, 4);
    let vertical = declination + declination_variation * (2.0 * random(index, 5) - 1.0);
    let horizontal = planar + planar_variation * (2.0 * random(index, 6) - 1.0);
    let velocity = [
        speed * vertical.sin() * horizontal.cos(),
        speed * vertical.sin() * horizontal.sin(),
        speed * vertical.cos(),
    ];
    let radius = source.timing.radius + source.radius_variation * random(index, 7);
    (position, velocity, radius.max(0.0))
}

pub(crate) fn process_shader(source: &RenderEmitter) -> String {
    let mut colour = String::from("vec4 colour_at(float t) {\n");
    if let Some((_, first)) = source.colours.first() {
        colour.push_str(&format!(
            "vec4 colour = vec4({:.9},{:.9},{:.9},{:.9});\n",
            first[0], first[1], first[2], first[3]
        ));
        for pair in source.colours.windows(2) {
            let ((a, _), (b, c)) = (pair[0], pair[1]);
            if b > a {
                colour.push_str(&format!(
                    "colour = mix(colour,vec4({:.9},{:.9},{:.9},{:.9}),clamp((t-{:.9})/{:.9},0.0,1.0));\n",
                    c[0],
                    c[1],
                    c[2],
                    c[3],
                    a,
                    b - a
                ));
            }
        }
    } else {
        let c = source.timing.color;
        colour.push_str(&format!(
            "vec4 colour = vec4({:.9},{:.9},{:.9},{:.9});\n",
            c[0], c[1], c[2], c[3]
        ));
    }
    if let Some((p, c)) = source.simple_colours {
        colour = format!("vec4 colour_at(float t) {{ vec4 a=vec4({:.9},{:.9},{:.9},{:.9}); vec4 b=vec4({:.9},{:.9},{:.9},{:.9}); vec4 c=vec4({:.9},{:.9},{:.9},{:.9}); vec3 rgb=b.rgb; if ({:.9}>{:.9}) {{ rgb=mix(a.rgb,b.rgb,clamp((t-{:.9})/max({:.9},0.000001),0.0,1.0)); }} if ({:.9}>{:.9}) {{ rgb=mix(rgb,c.rgb,clamp((t-{:.9})/max({:.9},0.000001),0.0,1.0)); }} float alpha=b.a; if ({:.9}>0.0) {{ alpha=mix(a.a,b.a,clamp(t/max({:.9},0.000001),0.0,1.0)); }} if ({:.9}>0.0) {{ alpha=mix(alpha,c.a,clamp((t-(1.0-{:.9}))/max({:.9},0.000001),0.0,1.0)); }} return vec4(rgb,alpha); }}\n",c[0][0],c[0][1],c[0][2],c[0][3],c[1][0],c[1][1],c[1][2],c[1][3],c[2][0],c[2][1],c[2][2],c[2][3],p[3],p[2],p[2],p[3]-p[2],p[5],p[4],p[4],p[5]-p[4],p[0],p[0],p[1],p[1],p[1]);
    } else {
        colour.push_str("return colour; }\n");
    }
    if source.scales.is_empty() {
        colour.push_str("float scale_at(float t) { return 1.0; }\n");
    } else {
        let values = source
            .scales
            .iter()
            .map(|f| format!("{f:.9}"))
            .collect::<Vec<_>>()
            .join(",");
        let n = source.scales.len();
        colour.push_str(&format!("const float SCALES[{n}] = float[{n}]({values});\nfloat scale_at(float t) {{ float p=clamp(t,0.0,1.0)*{:.9}; int i=int(floor(p)); return mix(SCALES[i],SCALES[min(i+1,{})],fract(p)); }}\n",(n-1) as f32,n-1));
    }
    format!("shader_type particles;\nrender_mode keep_data, disable_velocity;\nuniform vec3 gravity = vec3(0.0);\n{colour}\nvoid start() {{}}\nvoid process() {{\nCUSTOM.y += DELTA;\nACTIVE = CUSTOM.y < CUSTOM.x;\nVELOCITY += gravity * DELTA;\nTRANSFORM[3].xyz += VELOCITY * DELTA;\nfloat size = CUSTOM.z * {:.9} * scale_at(CUSTOM.y / max(CUSTOM.x,0.000001));\nif ({:.9} > 0.0) {{ size *= clamp(CUSTOM.y / max({:.9},0.000001),0.0,1.0); }}\nif ({:.9} > 0.0) {{ size *= clamp((CUSTOM.x-CUSTOM.y)/max({:.9},0.000001),0.0,1.0); }}\nTRANSFORM[0].xyz = vec3(size,0.0,0.0);\nTRANSFORM[1].xyz = vec3(0.0,size,0.0);\nTRANSFORM[2].xyz = vec3(0.0,0.0,size);\nCOLOR = colour_at(CUSTOM.y / max(CUSTOM.x,0.000001));\n}}", source.base_scale,source.grow_time,source.grow_time,source.fade_time,source.fade_time)
}

pub(crate) fn billboard_shader(code: &str, source: &RenderEmitter) -> String {
    let mut helpers = String::new();
    let mut vertex = String::new();
    if !source.atlas.is_empty() {
        let n = source.atlas.len();
        let values = source
            .atlas
            .iter()
            .map(|r| format!("vec4({:.9},{:.9},{:.9},{:.9})", r[0], r[1], r[2], r[3]))
            .collect::<Vec<_>>()
            .join(",");
        helpers.push_str(&format!("const vec4 ATLAS[{n}]=vec4[{n}]({values});\n"));
        let f = source.frames;
        vertex.push_str(&format!("float start_frame={:.9}+INSTANCE_CUSTOM.w*{:.9}; float fps={:.9}+fract(INSTANCE_CUSTOM.w*1.73)*{:.9}; float frame=floor(start_frame+INSTANCE_CUSTOM.y*fps); float end_frame=min({:.9},{:.9}); float loop_frame=clamp({:.9}+fract(INSTANCE_CUSTOM.w*3.13)*{:.9},0.0,end_frame); if(frame>end_frame) {{ frame=loop_frame+mod(frame-loop_frame,max(end_frame-loop_frame+1.0,1.0)); }} int frame_index=clamp(int(frame),0,{}); vec4 rect=ATLAS[frame_index]; UV=rect.xz+UV*rect.yw;\n",f[0],f[1],f[5],f[6],f[2],(n-1) as f32,f[3],f[4],n-1));
    }
    let r = source.rotation;
    vertex.push_str(&format!("float angle={:.9}+{:.9}*(INSTANCE_CUSTOM.w*2.0-1.0)+({:.9}+{:.9}*(fract(INSTANCE_CUSTOM.w*2.37)*2.0-1.0))*INSTANCE_CUSTOM.y; float c=cos(angle); float s=sin(angle); VERTEX.xy=mat2(vec2(c,s),vec2(-s,c))*VERTEX.xy;\n",r[2],r[3],r[0],r[1]));
    // The instance basis carries particle size and the camera supplies orientation.
    vertex.push_str("vec3 particle_scale=vec3(length(MODEL_MATRIX[0].xyz),length(MODEL_MATRIX[1].xyz),length(MODEL_MATRIX[2].xyz)); MODELVIEW_MATRIX=VIEW_MATRIX*mat4(vec4(INV_VIEW_MATRIX[0].xyz*particle_scale.x,0.0),vec4(INV_VIEW_MATRIX[1].xyz*particle_scale.y,0.0),vec4(INV_VIEW_MATRIX[2].xyz*particle_scale.z,0.0),MODEL_MATRIX[3]); MODELVIEW_NORMAL_MATRIX=mat3(MODELVIEW_MATRIX);\n");
    code.replace(
        "void vertex() {",
        &format!("{helpers}void vertex() {{\n{vertex}"),
    )
}

fn godot_transform(t: soe_nif::types::Transform) -> Transform3D {
    let t = placement_to_godot(t.rotation.rows, t.translation.to_array(), t.scale);
    transform3d_from(t)
}

impl CellStreamerNode {
    pub(super) fn add_nif_particle(
        &mut self,
        cell: GlobalFormId,
        source: &LoadedParticle,
        scenario: Rid,
    ) {
        let key = (source.reference, source.emitter.block);
        if self.nif_particles.contains_key(&key) {
            return;
        }
        let base = material_for(&mut self.materials, &source.material, &source.textures);
        let Ok(base) = base.try_cast::<ShaderMaterial>() else {
            return;
        };
        let Some(shader) = base.get_shader() else {
            return;
        };
        let shader_code = billboard_shader(&shader.get_code().to_string(), &source.emitter);
        let shader = shader_for_code(&self.materials.shaders, shader_code);
        let mut material = base.duplicate_resource();
        material.set_shader(&shader);
        let mut process = ShaderMaterial::new_gd();
        let shader = shader_for_code(&self.materials.shaders, process_shader(&source.emitter));
        process.set_shader(&shader);
        let gravity = if source.emitter.world_space {
            source
                .placement
                .rotation
                .apply(soe_nif::types::Vec3::new(
                    source.emitter.gravity[0],
                    source.emitter.gravity[1],
                    source.emitter.gravity[2],
                ))
                .to_array()
        } else {
            source.emitter.gravity
        };
        let g = direction_to_godot(gravity).map(|v| {
            v * crate::units::METRES_PER_UNIT
                * if source.emitter.world_space {
                    source.placement.scale
                } else {
                    1.0
                }
        });
        process.set_shader_parameter("gravity", &Vector3::new(g[0], g[1], g[2]).to_variant());
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(2.0, 2.0));
        quad.set_material(&material);
        let mut server = RenderingServer::singleton();
        let particles = server.particles_create();
        server.particles_set_amount(particles, source.emitter.timing.capacity as i32);
        server.particles_set_lifetime(
            particles,
            f64::from(
                source.emitter.timing.lifetime
                    + source.emitter.timing.lifetime_variation.abs()
                    + 1.0,
            ),
        );
        server.particles_set_emitting(particles, false);
        server.particles_set_use_local_coordinates(particles, !source.emitter.world_space);
        server.particles_set_process_material(particles, process.get_rid());
        server.particles_set_draw_passes(particles, 1);
        server.particles_set_draw_pass_mesh(particles, 0, quad.get_rid());
        let iso = if source.emitter.world_space {
            source.placement.then(&source.emitter.origin)
        } else {
            source.emitter.origin
        };
        let p = SkyrimPos::new(iso.translation.x, iso.translation.y, iso.translation.z).to_godot();
        server.particles_set_custom_aabb(
            particles,
            Aabb::new(
                Vector3::new(p.x - 20.0, p.y - 20.0, p.z - 20.0),
                Vector3::ONE * 40.0,
            ),
        );
        let rid = server.instance_create2(particles, scenario);
        server.instance_geometry_set_cast_shadows_setting(
            rid,
            if source.emitter.material.cast_shadows {
                godot::classes::rendering_server::ShadowCastingSetting::ON
            } else {
                godot::classes::rendering_server::ShadowCastingSetting::OFF
            },
        );
        if !source.emitter.world_space {
            server.instance_set_transform(rid, godot_transform(source.placement));
        }
        for unsupported in &source.emitter.unsupported {
            soe_trace::warn!(target: "host::particle", system = "host::particle", "{} {} unsupported modifier {unsupported}",source.reference,source.emitter.timing.system_name);
        }
        self.nif_particles.insert(
            key,
            ResidentParticle {
                cell,
                reference: source.reference,
                source: source.clone(),
                timeline: Timeline::new(source.emitter.timing.clone()),
                elapsed: 0.0,
                submitted: 0,
                rid,
                particles,
                material,
                _process: process,
                _quad: quad,
                placement: source.placement,
                enabled: true,
            },
        );
    }

    pub(super) fn pause_nif_particles(&mut self, speed: f64) {
        let mut server = RenderingServer::singleton();
        for particle in self.nif_particles.values() {
            server.particles_set_speed_scale(particle.particles, speed);
        }
    }

    pub(super) fn advance_nif_particles(&mut self, delta: f32) {
        if !delta.is_finite() || delta <= 0.0 {
            return;
        }
        let eye = self
            .base()
            .get_viewport()
            .and_then(|viewport| viewport.get_camera_3d())
            .map(|camera| camera.get_global_position());
        let mut server = RenderingServer::singleton();
        for resident in self.nif_particles.values_mut() {
            if !resident.enabled {
                resident.timeline.advance_disabled(delta);
                resident.elapsed += delta;
                continue;
            }
            let old = resident.elapsed;
            resident.timeline.advance(delta);
            resident.elapsed += delta;
            let source = &resident.source.emitter;
            let placement = if source.world_space {
                resident.placement.then(&source.origin)
            } else {
                source.origin
            };
            let (emit_scale, size_scale) = match eye {
                Some(eye) if source.lod.is_some() => {
                    let at = SkyrimPos::new(
                        placement.translation.x,
                        placement.translation.y,
                        placement.translation.z,
                    )
                    .to_godot();
                    let metres = eye.distance_to(Vector3::new(at.x, at.y, at.z));
                    lod_scales(source.lod, metres / crate::units::METRES_PER_UNIT)
                }
                _ => (1.0, 1.0),
            };
            for particle in resident
                .timeline
                .particles()
                .iter()
                .filter(|p| p.born_at > old)
            {
                if emit_scale < 1.0 && random(resident.submitted, 29) > emit_scale {
                    resident.submitted += 1;
                    continue;
                }
                let (position, velocity, radius) = spawn(source, resident.submitted);
                let position = placement.apply(soe_nif::types::Vec3::new(
                    position[0],
                    position[1],
                    position[2],
                ));
                let position = SkyrimPos::new(position.x, position.y, position.z).to_godot();
                let velocity = placement
                    .rotation
                    .apply(soe_nif::types::Vec3::new(
                        velocity[0],
                        velocity[1],
                        velocity[2],
                    ))
                    .scale(placement.scale)
                    .to_array();
                let velocity =
                    direction_to_godot(velocity).map(|v| v * crate::units::METRES_PER_UNIT);
                server.particles_emit(
                    resident.particles,
                    Transform3D::new(
                        Basis::IDENTITY,
                        Vector3::new(position.x, position.y, position.z),
                    ),
                    Vector3::new(velocity[0], velocity[1], velocity[2]),
                    Color::WHITE,
                    Color::from_rgba(
                        particle.lifetime,
                        resident.elapsed - particle.born_at,
                        radius * size_scale * placement.scale * crate::units::METRES_PER_UNIT,
                        random(resident.submitted, 23),
                    ),
                    31,
                );
                resident.submitted += 1;
            }
            server.particles_request_process(resident.particles);
        }
    }

    pub(super) fn sync_nif_particle_placements(&mut self, world: &soe_game::World) {
        let mut server = RenderingServer::singleton();
        for resident in self.nif_particles.values_mut() {
            let Some(placement) = world.ref_placement(resident.reference) else {
                continue;
            };
            resident.enabled =
                placement.visible && world.destruction_model(resident.reference).is_none();
            resident.placement = soe_nif::types::Transform {
                rotation: soe_nif::types::Mat33 {
                    rows: crate::units::skyrim_euler_matrix(placement.rotation),
                },
                translation: soe_nif::types::Vec3::new(
                    placement.position[0],
                    placement.position[1],
                    placement.position[2],
                ),
                scale: placement.scale,
            };
            server.instance_set_visible(resident.rid, resident.enabled);
            if !resident.source.emitter.world_space {
                server.instance_set_transform(resident.rid, godot_transform(resident.placement));
            }
        }
    }

    pub(super) fn nif_particle_surfaces(&self) -> Array<VarDictionary> {
        self.nif_particles
            .values()
            .map(|p| {
                let mut row = VarDictionary::new();
                let center = p.placement.then(&p.source.emitter.origin).translation;
                let center = SkyrimPos::new(center.x, center.y, center.z).to_godot();
                row.set(
                    "bounds",
                    Aabb::new(
                        Vector3::new(center.x - 20.0, center.y - 20.0, center.z - 20.0),
                        Vector3::ONE * 40.0,
                    ),
                );
                row.set("animated", true);
                row.set("effect", true);
                row.set("refraction", false);
                row.set("clamp_mode", -1_i64);
                row.set("flags1", i64::from(p.source.material.shader_flags_1));
                row.set("flags2", i64::from(p.source.material.shader_flags_2));
                let effect = p.source.material.effect.as_ref();
                row.set(
                    "palette",
                    effect.map_or("", |e| e.params.greyscale_texture.as_str()),
                );
                row.set(
                    "src_blend",
                    effect.and_then(|e| e.blend).map_or(-1, |b| b.src as i64),
                );
                row.set(
                    "dst_blend",
                    effect.and_then(|e| e.blend).map_or(-1, |b| b.dst as i64),
                );
                row.set("models", &Array::<GString>::new());
                row.set("particle", true);
                row.set("rid", p.rid);
                row.set("material", &p.material);
                row.set(
                    "particle_system",
                    p.source.emitter.timing.system_name.as_str(),
                );
                row.set(
                    "source",
                    p.source.material.diffuse.as_deref().unwrap_or_default(),
                );
                row.set(
                    "references",
                    &Array::from_iter([GString::from(p.reference.to_string().as_str())]),
                );
                row.set("nominal_rate", p.source.emitter.timing.rate.value);
                row.set("elapsed", p.elapsed);
                row.set("emitted", p.submitted as i64);
                row.set("alive", p.timeline.alive() as i64);
                row.set("capacity", p.source.emitter.timing.capacity as i64);
                row
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> RenderEmitter {
        RenderEmitter {
            timing: soe_nif::particle::Emitter {
                name: "Emitter".into(),
                system_name: "Fixture".into(),
                transform: soe_nif::types::Transform::IDENTITY,
                rate: soe_nif::particle::Rate {
                    time: soe_nif::controller::TimeController::default(),
                    value: 4.0,
                    keys: None,
                    visibility: soe_nif::particle::Visibility {
                        default: true,
                        keys: Vec::new(),
                    },
                },
                lifetime: 1.0,
                lifetime_variation: 0.0,
                radius: 2.0,
                color: [0.0, 1.0, 0.0, 1.0],
                capacity: 4,
            },
            block: 0,
            material: soe_nif::mesh::Material::default(),
            origin: soe_nif::types::Transform::IDENTITY,
            world_space: true,
            velocity: [24.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            volume: Volume::Cylinder {
                radius: 9.0,
                height: 11.0,
            },
            radius_variation: 0.0,
            grow_time: 0.1,
            fade_time: 0.1,
            base_scale: 1.0,
            colours: Vec::new(),
            gravity: [0.0; 3],
            simple_colours: None,
            scales: Vec::new(),
            atlas: Vec::new(),
            frames: [0.0; 7],
            aspect: 1.0,
            rotation: [0.0; 4],
            random_rotation_sign: false,
            drag: [0.0; 3],
            lod: None,
            unsupported: Vec::new(),
        }
    }

    #[test]
    fn native_particle_births_stay_inside_authored_volume_with_authored_speed() {
        let source = fixture();
        for i in 0..1024 {
            let (p, v, r) = spawn(&source, i);
            assert!(p[0] * p[0] + p[1] * p[1] <= 81.0);
            assert!(p[2].abs() <= 5.5);
            assert_eq!(v, [0.0, 0.0, 24.0]);
            assert_eq!(r, 2.0);
        }
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/r4-particle-fixture.gdshader");
        std::fs::write(path, process_shader(&source)).unwrap();
        let shader = "shader_type spatial; render_mode unshaded,cull_disabled; void vertex() {} void fragment(){ ALBEDO=COLOR.rgb; }";
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/r4-particle-fixture-draw.gdshader");
        std::fs::write(path, billboard_shader(shader, &source)).unwrap();
    }
    #[test]
    fn installed_particle_asset_exports_native_shaders_and_preserves_atlas_counts() {
        let Some(directory) = std::env::var_os("SOE_NIF_SAMPLES") else {
            return;
        };
        let path = Path::new(&directory).join("meshes/clutter/giant/giantcampfire01burning.nif");
        let file = soe_nif::parse(&std::fs::read(path).unwrap()).unwrap();
        let emitters = soe_nif::particle::render_emitters(&file).unwrap();
        assert_eq!(emitters.len(), 3);
        for (source, (atlas, scales, speed)) in
            emitters
                .iter()
                .zip([(64, 220, 0.0), (32, 60, 60.0), (16, 200, 45.0)])
        {
            assert_eq!(source.atlas.len(), atlas);
            assert_eq!(source.scales.len(), scales);
            assert_eq!(source.velocity[0], speed);
            let desc = crate::data_source::particle_material(source);
            let draw = crate::nif_effect::shader(&desc, desc.effect.as_ref().unwrap());
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
            std::fs::write(
                root.join(format!("r4-particle-{}.gdshader", source.block)),
                process_shader(source),
            )
            .unwrap();
            std::fs::write(
                root.join(format!("r4-particle-{}-draw.gdshader", source.block)),
                billboard_shader(&draw, source),
            )
            .unwrap();
        }
    }
}

#[cfg(test)]
mod lod_tests {
    use super::lod_scales;

    #[test]
    fn lod_modifier_scales_births_and_size_with_distance() {
        let lod = Some([0.1, 0.7, 0.2, 2.0]);
        assert_eq!(lod_scales(None, 9999.0), (1.0, 1.0));
        assert_eq!(lod_scales(lod, 0.0), (1.0, 1.0));
        let (emit, size) = lod_scales(lod, 0.4 * 4096.0);
        assert!((emit - 0.6).abs() < 1e-4 && (size - 1.5).abs() < 1e-4);
        assert_eq!(lod_scales(lod, 5.0 * 4096.0), (0.2, 2.0));
    }
}
