// SPDX-License-Identifier: GPL-3.0-or-later

//! A NIF writer for the tests, written from nif.xml independently of the
//! reader so a layout mistake on either side shows up as a mismatch. It
//! covers exactly the blocks the reader parses plus opaque junk blocks.

#![allow(dead_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::manual_midpoint
)]

use soe_nif::stream::f32_to_half;
use soe_nif::vertex::{attr, VertexDesc};

pub const VERSION: u32 = 0x1402_0007;

/// Little-endian byte builder.
#[derive(Default, Clone)]
pub struct Buf(pub Vec<u8>);

impl Buf {
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(v as u8)
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn f32(&mut self, v: f32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn half(&mut self, v: f32) -> &mut Self {
        self.u16(f32_to_half(v))
    }
    pub fn vec3(&mut self, v: [f32; 3]) -> &mut Self {
        self.f32(v[0]).f32(v[1]).f32(v[2])
    }
    pub fn floats(&mut self, v: &[f32]) -> &mut Self {
        for &f in v {
            self.f32(f);
        }
        self
    }
    /// Matrix33 is stored column by column.
    pub fn mat33(&mut self, rows: [[f32; 3]; 3]) -> &mut Self {
        for c in 0..3 {
            for row in rows.iter() {
                self.f32(row[c]);
            }
        }
        self
    }
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }
    pub fn sized_string(&mut self, s: &str) -> &mut Self {
        self.u32(s.len() as u32).bytes(s.as_bytes())
    }
    pub fn export_string(&mut self, s: &str) -> &mut Self {
        self.u8(s.len() as u8 + 1).bytes(s.as_bytes()).u8(0)
    }
    pub fn refs(&mut self, r: &[i32]) -> &mut Self {
        self.u32(r.len() as u32);
        for &x in r {
            self.i32(x);
        }
        self
    }
}

pub const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Rotation of 90 degrees about Z: x goes to y.
pub const ROT_Z_90: [[f32; 3]; 3] = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];

pub struct NifWriter {
    pub bs_version: u32,
    pub strings: Vec<String>,
    pub blocks: Vec<(String, Vec<u8>)>,
    pub roots: Vec<i32>,
}

impl NifWriter {
    pub fn new(bs_version: u32) -> NifWriter {
        NifWriter {
            bs_version,
            strings: Vec::new(),
            blocks: Vec::new(),
            roots: Vec::new(),
        }
    }

    pub fn string(&mut self, s: &str) -> u32 {
        if let Some(i) = self.strings.iter().position(|x| x == s) {
            return i as u32;
        }
        self.strings.push(s.to_string());
        (self.strings.len() - 1) as u32
    }

    pub fn add(&mut self, type_name: &str, data: Vec<u8>) -> i32 {
        self.blocks.push((type_name.to_string(), data));
        (self.blocks.len() - 1) as i32
    }

    /// Reserve a block index to fill in later, for forward references.
    pub fn reserve(&mut self, type_name: &str) -> i32 {
        self.add(type_name, Vec::new())
    }

    pub fn fill(&mut self, index: i32, data: Vec<u8>) {
        self.blocks[index as usize].1 = data;
    }

    pub fn finish(&self) -> Vec<u8> {
        let mut b = Buf::default();
        b.bytes(b"Gamebryo File Format, Version 20.2.0.7\n");
        b.u32(VERSION).u8(1).u32(12).u32(self.blocks.len() as u32);
        b.u32(self.bs_version);
        b.export_string("soe-nif tests");
        if self.bs_version > 130 {
            b.u32(0);
        } else {
            b.export_string("");
        }
        b.export_string("");
        if self.bs_version >= 103 {
            b.export_string("");
        }
        let mut types: Vec<String> = Vec::new();
        for (t, _) in &self.blocks {
            if !types.contains(t) {
                types.push(t.clone());
            }
        }
        b.u16(types.len() as u16);
        for t in &types {
            b.sized_string(t);
        }
        for (t, _) in &self.blocks {
            b.u16(types.iter().position(|x| x == t).unwrap() as u16);
        }
        for (_, data) in &self.blocks {
            b.u32(data.len() as u32);
        }
        b.u32(self.strings.len() as u32);
        b.u32(self.strings.iter().map(String::len).max().unwrap_or(0) as u32);
        for s in &self.strings {
            b.sized_string(s);
        }
        b.u32(0);
        for (_, data) in &self.blocks {
            b.bytes(data);
        }
        b.refs(&self.roots);
        b.0
    }

    // Block builders. Each returns the block's bytes for `add`.

    pub fn object_net(&mut self, b: &mut Buf, name: &str, extra: &[i32]) {
        let idx = if name.is_empty() {
            u32::MAX
        } else {
            self.string(name)
        };
        b.u32(idx).refs(extra).i32(-1);
    }

    pub fn av_object(
        &mut self,
        b: &mut Buf,
        name: &str,
        translation: [f32; 3],
        rotation: [[f32; 3]; 3],
        scale: f32,
        collision: i32,
    ) {
        self.object_net(b, name, &[]);
        b.u32(0xE).vec3(translation).mat33(rotation).f32(scale);
        b.i32(collision);
    }

    pub fn ni_node(
        &mut self,
        name: &str,
        translation: [f32; 3],
        rotation: [[f32; 3]; 3],
        scale: f32,
        children: &[i32],
    ) -> Vec<u8> {
        let mut b = Buf::default();
        self.av_object(&mut b, name, translation, rotation, scale, -1);
        b.refs(children);
        if self.bs_version < 130 {
            b.u32(0);
        }
        b.0
    }

    pub fn texture_set(&self, textures: &[&str]) -> Vec<u8> {
        let mut b = Buf::default();
        b.u32(textures.len() as u32);
        for t in textures {
            b.sized_string(t);
        }
        b.0
    }

    /// BSLightingShaderProperty for Skyrim streams. `tail` is the per-type
    /// float tail (for example the environment map scale for type 1).
    pub fn lighting_shader(
        &mut self,
        shader_type: u32,
        flags1: u32,
        flags2: u32,
        texture_set: i32,
        tail: &[f32],
    ) -> Vec<u8> {
        let mut b = Buf::default();
        b.u32(shader_type);
        self.object_net(&mut b, "", &[]);
        b.u32(flags1).u32(flags2);
        b.floats(&[0.0, 0.0, 1.0, 1.0]);
        b.i32(texture_set);
        b.floats(&[0.0, 0.0, 0.0, 1.0]);
        if self.bs_version >= 130 {
            b.u32(u32::MAX);
        }
        b.u32(3).f32(1.0).f32(0.0).f32(80.0);
        b.floats(&[1.0, 1.0, 1.0, 1.0]);
        if self.bs_version >= 130 {
            // Fallout 4 has a longer tail the reader skips by size.
            b.floats(&[0.0; 12]);
            return b.0;
        }
        b.floats(&[0.3, 2.0]);
        b.floats(tail);
        b.0
    }

    pub fn effect_shader(
        &mut self,
        flags1: u32,
        flags2: u32,
        source: &str,
        base_color: [f32; 4],
    ) -> Vec<u8> {
        let mut b = Buf::default();
        self.object_net(&mut b, "", &[]);
        b.u32(flags1).u32(flags2);
        b.floats(&[0.0, 0.0, 1.0, 1.0]);
        b.sized_string(source);
        b.u8(3).u8(255).u8(0).u8(0);
        b.floats(&[1.0, 1.0, 0.75, 0.25]);
        b.floats(&base_color);
        b.f32(2.0).f32(100.0);
        b.sized_string("textures\\effects\\grey.dds");
        b.0
    }

    pub fn alpha_property(&mut self, flags: u16, threshold: u8) -> Vec<u8> {
        let mut b = Buf::default();
        self.object_net(&mut b, "", &[]);
        b.u16(flags).u8(threshold);
        b.0
    }

    pub fn write_vertex(&self, b: &mut Buf, attrs: u16, v: &TestVertex) {
        if attrs & attr::VERTEX != 0 {
            let full = self.bs_version < 130 || attrs & attr::FULL_PRECISION != 0;
            if full {
                b.vec3(v.position);
                if attrs & attr::TANGENTS != 0 {
                    b.f32(v.bitangent[0]);
                } else {
                    b.u32(0);
                }
            } else {
                b.half(v.position[0])
                    .half(v.position[1])
                    .half(v.position[2]);
                if attrs & attr::TANGENTS != 0 {
                    b.half(v.bitangent[0]);
                } else {
                    b.u16(0);
                }
            }
        }
        if attrs & attr::UVS != 0 {
            b.half(v.uv[0]).half(v.uv[1]);
        }
        if attrs & attr::NORMALS != 0 {
            for c in v.normal {
                b.u8(norm_byte(c));
            }
            b.u8(norm_byte(v.bitangent[1]));
            if attrs & attr::TANGENTS != 0 {
                for c in v.tangent {
                    b.u8(norm_byte(c));
                }
                b.u8(norm_byte(v.bitangent[2]));
            }
        }
        if attrs & attr::COLORS != 0 {
            b.bytes(&v.color);
        }
        if attrs & attr::SKINNED != 0 {
            for w in v.weights {
                b.half(w);
            }
            b.bytes(&v.bone_indices);
        }
        if attrs & attr::EYE_DATA != 0 {
            b.f32(0.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn bs_tri_shape(
        &mut self,
        name: &str,
        translation: [f32; 3],
        skin: i32,
        shader: i32,
        alpha: i32,
        attrs: u16,
        vertices: &[TestVertex],
        triangles: &[[u16; 3]],
        inline_data: bool,
    ) -> Vec<u8> {
        let mut b = Buf::default();
        self.av_object(&mut b, name, translation, IDENTITY, 1.0, -1);
        b.vec3([0.0, 0.0, 0.0]).f32(1.0);
        b.i32(skin).i32(shader).i32(alpha);
        let desc = VertexDesc::from_attributes(attrs, self.bs_version);
        b.u64(desc.0);
        if self.bs_version >= 130 {
            b.u32(triangles.len() as u32);
        } else {
            b.u16(triangles.len() as u16);
        }
        b.u16(vertices.len() as u16);
        if inline_data {
            b.u32((desc.size_bytes() * vertices.len() + 6 * triangles.len()) as u32);
            for v in vertices {
                self.write_vertex(&mut b, attrs, v);
            }
            for t in triangles {
                b.u16(t[0]).u16(t[1]).u16(t[2]);
            }
        } else {
            b.u32(0);
        }
        if self.bs_version == 100 {
            b.u32(0);
        }
        b.0
    }

    pub fn ni_tri_shape(
        &mut self,
        name: &str,
        data: i32,
        skin: i32,
        shader: i32,
        alpha: i32,
    ) -> Vec<u8> {
        let mut b = Buf::default();
        self.av_object(&mut b, name, [0.0; 3], IDENTITY, 1.0, -1);
        b.i32(data).i32(skin);
        b.u32(0).i32(-1).u8(0);
        b.i32(shader).i32(alpha);
        b.0
    }

    /// NiTriShapeData (or NiTriStripsData when `strips` is given).
    pub fn geometry_data(
        &self,
        vertices: &[TestVertex],
        with_tangents: bool,
        triangles: &[[u16; 3]],
        strips: Option<&[Vec<u16>]>,
    ) -> Vec<u8> {
        let mut b = Buf::default();
        b.i32(0).u16(vertices.len() as u16).u8(0).u8(0);
        b.bool(true);
        for v in vertices {
            b.vec3(v.position);
        }
        let flags: u16 = 1 | if with_tangents { 4096 } else { 0 };
        b.u16(flags);
        if self.bs_version > 34 {
            b.u32(0);
        }
        b.bool(true);
        for v in vertices {
            b.vec3(v.normal);
        }
        if with_tangents {
            for v in vertices {
                b.vec3(v.tangent);
            }
            for v in vertices {
                b.vec3(v.bitangent);
            }
        }
        b.vec3([0.0; 3]).f32(1.0);
        b.bool(true);
        for v in vertices {
            for c in v.color {
                b.f32(c as f32 / 255.0);
            }
        }
        for v in vertices {
            b.f32(v.uv[0]).f32(v.uv[1]);
        }
        b.u16(0).i32(-1);
        match strips {
            None => {
                b.u16(triangles.len() as u16);
                b.u32(triangles.len() as u32 * 3);
                b.bool(true);
                for t in triangles {
                    b.u16(t[0]).u16(t[1]).u16(t[2]);
                }
                b.u16(0);
            }
            Some(strips) => {
                let nt: usize = strips.iter().map(|s| s.len().saturating_sub(2)).sum();
                b.u16(nt as u16);
                b.u16(strips.len() as u16);
                for s in strips {
                    b.u16(s.len() as u16);
                }
                b.bool(true);
                for s in strips {
                    for &i in s {
                        b.u16(i);
                    }
                }
            }
        }
        b.0
    }

    pub fn skin_instance(
        &self,
        data: i32,
        partition: i32,
        root: i32,
        bones: &[i32],
        body_parts: Option<&[(u16, u16)]>,
    ) -> Vec<u8> {
        let mut b = Buf::default();
        b.i32(data).i32(partition).i32(root).refs(bones);
        if let Some(parts) = body_parts {
            b.u32(parts.len() as u32);
            for &(f, p) in parts {
                b.u16(f).u16(p);
            }
        }
        b.0
    }

    /// NiSkinData with one bind transform per bone and optional weight lists.
    pub fn skin_data(&self, bones: &[TestBone], with_weights: bool) -> Vec<u8> {
        let mut b = Buf::default();
        b.mat33(IDENTITY).vec3([0.0; 3]).f32(1.0);
        b.u32(bones.len() as u32).bool(with_weights);
        for (translation, weights) in bones {
            b.mat33(IDENTITY).vec3(*translation).f32(1.0);
            b.vec3([0.0; 3]).f32(1.0);
            b.u16(weights.len() as u16);
            if with_weights {
                for &(v, w) in weights {
                    b.u16(v).f32(w);
                }
            }
        }
        b.0
    }
}

/// A skin bone for `skin_data`: bind translation and (vertex, weight) list.
pub type TestBone = ([f32; 3], Vec<(u16, f32)>);

pub struct TestPartition {
    pub bones: Vec<u16>,
    pub vertex_map: Vec<u16>,
    pub bones_per_vertex: u16,
    /// `vertex_map.len() * bones_per_vertex` entries.
    pub weights: Vec<f32>,
    pub bone_indices: Vec<u8>,
    pub triangles: Vec<[u16; 3]>,
    /// SSE only: the same triangles in shape indices.
    pub triangles_copy: Vec<[u16; 3]>,
}

impl NifWriter {
    /// NiSkinPartition. For SSE, `global` carries the shape's vertex buffer.
    pub fn skin_partition(
        &self,
        partitions: &[TestPartition],
        global: Option<(u16, &[TestVertex])>,
    ) -> Vec<u8> {
        let mut b = Buf::default();
        b.u32(partitions.len() as u32);
        let mut desc = VertexDesc(0);
        if self.bs_version == 100 {
            match global {
                Some((attrs, vertices)) => {
                    desc = VertexDesc::from_attributes(attrs, 100);
                    b.u32((desc.size_bytes() * vertices.len()) as u32);
                    b.u32(desc.size_bytes() as u32);
                    b.u64(desc.0);
                    for v in vertices {
                        self.write_vertex(&mut b, attrs, v);
                    }
                }
                None => {
                    b.u32(0).u32(0).u64(0);
                }
            }
        }
        for p in partitions {
            b.u16(p.vertex_map.len() as u16);
            b.u16(p.triangles.len() as u16);
            b.u16(p.bones.len() as u16);
            b.u16(0);
            b.u16(p.bones_per_vertex);
            for &bone in &p.bones {
                b.u16(bone);
            }
            b.bool(true);
            for &v in &p.vertex_map {
                b.u16(v);
            }
            b.bool(!p.weights.is_empty());
            for &w in &p.weights {
                b.f32(w);
            }
            b.bool(true);
            for t in &p.triangles {
                b.u16(t[0]).u16(t[1]).u16(t[2]);
            }
            b.bool(!p.bone_indices.is_empty());
            b.bytes(&p.bone_indices);
            if self.bs_version > 34 {
                b.u8(0).bool(global.is_some());
                if self.bs_version == 100 {
                    b.u64(desc.0);
                    for t in &p.triangles_copy {
                        b.u16(t[0]).u16(t[1]).u16(t[2]);
                    }
                }
            }
        }
        b.0
    }
}

pub fn norm_byte(v: f32) -> u8 {
    (f32::midpoint(v, 1.0) * 255.0).round().clamp(0.0, 255.0) as u8
}

#[derive(Clone, Debug)]
pub struct TestVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub tangent: [f32; 3],
    pub bitangent: [f32; 3],
    pub color: [u8; 4],
    pub weights: [f32; 4],
    pub bone_indices: [u8; 4],
}

impl TestVertex {
    pub fn at(position: [f32; 3], uv: [f32; 2]) -> TestVertex {
        TestVertex {
            position,
            uv,
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0],
            bitangent: [0.0, 1.0, 0.0],
            color: [255, 128, 0, 255],
            weights: [1.0, 0.0, 0.0, 0.0],
            bone_indices: [0, 0, 0, 0],
        }
    }
}

/// A unit quad in the XY plane, two triangles, UVs exactly representable
/// as halves.
pub fn quad() -> (Vec<TestVertex>, Vec<[u16; 3]>) {
    let v = vec![
        TestVertex::at([0.0, 0.0, 0.0], [0.0, 0.0]),
        TestVertex::at([1.0, 0.0, 0.0], [1.0, 0.0]),
        TestVertex::at([1.0, 1.0, 0.0], [1.0, 1.0]),
        TestVertex::at([0.0, 1.0, 0.0], [0.0, 1.0]),
    ];
    (v, vec![[0, 1, 2], [0, 2, 3]])
}
