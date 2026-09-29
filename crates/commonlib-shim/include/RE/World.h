// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <array>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
enum class UnsupportedHook { ActivateReferenceChildren };
class Calendar {
public:
    virtual ~Calendar() = default;
    virtual std::uint32_t year() const = 0;
    virtual std::uint32_t month() const = 0;
    virtual std::uint32_t day() const = 0;
    virtual double hour() const = 0;
    virtual double days_passed() const = 0;
    virtual double timescale() const = 0;
};
class Character {
public:
    virtual ~Character() = default;
    virtual FormId form_id() const = 0;
    virtual std::optional<float> actor_value(const std::string& name) const = 0;
    virtual bool is_dead() const = 0;
    virtual std::optional<FormId> race() const = 0;
};
class DEFAULT_OBJECTS {
public:
    virtual ~DEFAULT_OBJECTS() = default;
    struct Result { bool success; std::optional<FormId> form; std::string error; };
    virtual Result get(const std::array<char, 4>& key) const = 0;
};
struct AliasInstance { FormId quest; std::uint32_t alias_id; std::optional<FormId> reference; std::vector<FormId> packages; };
class ExtraAliasInstanceArray {
public:
    virtual ~ExtraAliasInstanceArray() = default;
    virtual std::vector<AliasInstance> instances(FormId reference) const = 0;
};
class ExtraActivateRefChildren {
public:
    virtual ~ExtraActivateRefChildren() = default;
    struct ChildrenResult { bool success; std::vector<FormId> children; UnsupportedHook error; };
    struct TimerResult { bool success; float seconds; UnsupportedHook error; };
    virtual ChildrenResult children(FormId reference) const = 0;
    virtual TimerResult timer(FormId reference) const = 0;
    virtual std::optional<UnsupportedHook> activate_children(FormId reference) const = 0;
};
} // namespace RE
