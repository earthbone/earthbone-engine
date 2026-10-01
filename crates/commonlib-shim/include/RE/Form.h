// Generated from crates/commonlib-shim/src/form.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
struct FormId {
    std::uint32_t value;
    static constexpr std::uint32_t LIGHT_PREFIX = 0xFE;
    static constexpr std::uint32_t LIGHT_LOCAL_MASK = 0x0FFF;
    std::optional<std::uint8_t> slot() const noexcept {
        const auto top = static_cast<std::uint8_t>(value >> 24);
        return top == LIGHT_PREFIX ? std::nullopt : std::optional<std::uint8_t>{top};
    }
    std::optional<std::uint16_t> light_slot() const noexcept {
        return (value >> 24) == LIGHT_PREFIX
            ? std::optional<std::uint16_t>{static_cast<std::uint16_t>((value >> 12) & 0x0FFF)}
            : std::nullopt;
    }
    std::uint32_t local() const noexcept {
        return light_slot() ? value & LIGHT_LOCAL_MASK : value & 0x00FFFFFF;
    }
    bool is_light() const noexcept { return light_slot().has_value(); }
};

struct FormType {
    char value[4];
    static constexpr FormType new_type(const char (&sig)[5]) noexcept {
        return {{sig[0], sig[1], sig[2], sig[3]}};
    }
    std::string as_str() const { return std::string(value, 4); }
};

class Form {
public:
    virtual ~Form() = default;
    virtual FormId form_id() const = 0;
    virtual FormType form_type() const = 0;
    virtual std::optional<std::string> editor_id() const { return std::nullopt; }
    virtual std::vector<FormId> keywords() const { return {}; }
};

struct FileInfo {
    std::string name;
    std::optional<std::uint8_t> regular_index;
    std::optional<std::uint16_t> light_index;
    bool is_light() const noexcept { return light_index.has_value(); }
};
class DataHandler {
public:
    virtual ~DataHandler() = default;
    virtual const Form* lookup(FormId id) const = 0;
    virtual const Form* lookup_by_editor_id(const std::string& editor_id) const = 0;
    virtual std::vector<FormId> forms_of_type(FormType kind) const = 0;
    virtual std::vector<std::string> files() const = 0;
    virtual std::optional<FileInfo> file(std::size_t index) const = 0;
};
using TESDataHandler = DataHandler;
using TESFile = FileInfo;
} // namespace RE
