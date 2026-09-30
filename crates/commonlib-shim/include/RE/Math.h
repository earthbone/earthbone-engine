// Generated from crates/commonlib-shim/src/ni.rs. Do not edit.
#pragma once
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct NiPoint3 {
    float x{};
    float y{};
    float z{};
    constexpr NiPoint3() = default;
    constexpr NiPoint3(float x_value, float y_value, float z_value) : x(x_value), y(y_value), z(z_value) {}
    float length() const;
    float dot(NiPoint3 other) const;
    constexpr NiPoint3 operator-(NiPoint3 other) const {
        return {x - other.x, y - other.y, z - other.z};
    }
};

struct NiMatrix3 {
    float value[3][3];
    NiMatrix3() : value{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}} {}
    NiPoint3 apply(NiPoint3 point) const;
};

struct NiTransform {
    NiMatrix3 rotate{};
    NiPoint3 translate{};
    float scale{1.0F};
    NiPoint3 apply(NiPoint3 point) const;
};

struct SceneNode { std::uint64_t value; };
class SceneGraph {
public:
    virtual ~SceneGraph() = default;
    virtual std::optional<SceneNode> find_by_name(SceneNode root, const std::string& name) const = 0;
    virtual NiTransform world_transform(SceneNode node) const = 0;
    virtual void set_local_transform(SceneNode node, NiTransform transform) = 0;
};
} // namespace RE
