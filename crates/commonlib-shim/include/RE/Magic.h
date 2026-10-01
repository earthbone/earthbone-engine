// Generated from the SHM-13 MIT runtime contract. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Reference.h"
#include "RE/WorldTypes.h"
#include <cstddef>
#include <cstdint>
#include <optional>

namespace RE {
class MagicFavorites {
public:
    virtual ~MagicFavorites() = default;
    virtual bool is_favorite(FormId form) const = 0;
    virtual bool set_favorite(FormId form, bool favorite) = 0;
};

enum class CannotCastReason : std::uint8_t {
    Ok = 0, Magicka = 1, PowerUsed = 2, RangedUnderWater = 3,
    MultipleCast = 4, ItemCharge = 5, CastWhileShouting = 6,
    ShoutWhileCasting = 7, ShoutWhileRecovering = 8
};
class MagicSystem {
public:
    virtual ~MagicSystem() = default;
    virtual CannotCastReason cannot_cast(FormId actor, FormId spell) const = 0;
};

class MagicTarget {
public:
    virtual ~MagicTarget() = default;
    virtual bool has_magic_effect(FormId effect) const = 0;
    virtual std::size_t active_effect_count() const = 0;
    virtual bool dispel_spell(FormId spell) = 0;
};

struct MapMarkerData {
    enum Flag : std::uint8_t { Visible = 1, CanTravelTo = 2, ShowAllHidden = 4 };
    std::uint8_t flags{};
    bool visible() const noexcept { return (flags & Visible) != 0; }
    bool can_travel_to() const noexcept { return (flags & CanTravelTo) != 0; }
    void set_visible(bool value) noexcept { flags = value ? flags | Visible : flags & ~Visible; }
    void set_can_travel_to(bool value) noexcept { flags = value ? flags | CanTravelTo : flags & ~CanTravelTo; }
};

struct ObjectEquipParams {
    std::int32_t count{1};
    bool play_equip_sounds{true};
    bool force_equip{};
    bool show_message{};
    bool has_extra_data{};
    bool has_equip_slot{};
    std::optional<Unsupported> unsupported() const noexcept {
        if (has_extra_data || has_equip_slot || force_equip || show_message || !play_equip_sounds || count != 1) {
            return Unsupported{UnsupportedHook::UnsupportedEquipOption};
        }
        return std::nullopt;
    }
};
} // namespace RE
