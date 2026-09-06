// SPDX-License-Identifier: MIT

//! NetImmerse math types. These are shim-provided values, the scene graph
//! itself is a Godot tree behind opaque node handles.

/// `RE::NiPoint3`, Skyrim units, Z up.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct NiPoint3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl NiPoint3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
}

impl std::ops::Sub for NiPoint3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

/// `RE::NiMatrix3`, row major as the game stores it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NiMatrix3(pub [[f32; 3]; 3]);

impl Default for NiMatrix3 {
    fn default() -> Self {
        Self([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]])
    }
}

impl NiMatrix3 {
    pub fn apply(&self, p: NiPoint3) -> NiPoint3 {
        let m = &self.0;
        NiPoint3::new(
            m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
            m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
            m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
        )
    }
}

/// `RE::NiTransform`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NiTransform {
    pub rotate: NiMatrix3,
    pub translate: NiPoint3,
    pub scale: f32,
}

impl Default for NiTransform {
    fn default() -> Self {
        Self {
            rotate: NiMatrix3::default(),
            translate: NiPoint3::default(),
            scale: 1.0,
        }
    }
}

impl NiTransform {
    pub fn apply(&self, p: NiPoint3) -> NiPoint3 {
        let r = self.rotate.apply(p);
        NiPoint3::new(
            r.x * self.scale + self.translate.x,
            r.y * self.scale + self.translate.y,
            r.z * self.scale + self.translate.z,
        )
    }
}

/// `RE::NiAVObject` and `RE::NiNode` as the shim sees them: a handle into
/// the host scene, looked up by the node name a plugin asks for.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SceneNode(pub u64);

pub trait SceneGraph {
    fn find_by_name(&self, root: SceneNode, name: &str) -> Option<SceneNode>;
    fn world_transform(&self, node: SceneNode) -> NiTransform;
    fn set_local_transform(&mut self, node: SceneNode, t: NiTransform);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transform_scales_then_translates() {
        let t = NiTransform {
            rotate: NiMatrix3::default(),
            translate: NiPoint3::new(1.0, 2.0, 3.0),
            scale: 2.0,
        };
        assert_eq!(t.apply(NiPoint3::new(1.0, 1.0, 1.0)), NiPoint3::new(3.0, 4.0, 5.0));
        assert_eq!((NiPoint3::new(3.0, 4.0, 0.0) - NiPoint3::default()).length(), 5.0);
    }
}
