// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
enum class SIT_SLEEP_STATE : std::uint32_t {
    kNormal = 0, kWantToSit = 1, kWaitingForSitAnim = 2, kIsSitting = 3,
    kRidingMount = kIsSitting,
    kWantToStand = 4, kWantToSleep = 5, kWaitingForSleepAnim = 6,
    kIsSleeping = 7, kWantToWake = 8
};
struct SettingValue {
    enum class Type { Bool, Integer, Float, String } type;
    bool boolean{};
    std::int32_t integer{};
    float real{};
    std::string string;
};
struct Setting { std::string name; SettingValue value; };
struct Sky { float hour; };
struct ReanimateEffect {
    std::uint64_t id;
    FormId spell;
    FormId effect;
    FormId target;
    float magnitude;
    double elapsed;
    std::optional<double> duration;
};
struct UnsupportedWorldHook { const char* name; const char* reason; };
class TES {
public:
    virtual ~TES() = default;
    virtual std::optional<Setting> setting(const std::string& name) const = 0;
    virtual Sky sky() const = 0;
    virtual std::optional<ReanimateEffect> reanimate_effect(std::uint64_t id) const = 0;
    virtual bool sit_sleep_state(FormId actor, SIT_SLEEP_STATE& state,
                                 UnsupportedWorldHook& error) const = 0;
    virtual UnsupportedWorldHook hook(const std::string& name) const = 0;
};
} // namespace RE
