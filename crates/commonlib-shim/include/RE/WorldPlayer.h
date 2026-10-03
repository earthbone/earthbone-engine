// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Reference.h"
#include "RE/WorldTypes.h"
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace RE {
struct FormKey { std::uint16_t file; std::uint32_t object; };
struct ProjectileHandle { std::uint32_t value; };
struct Position { float x; float y; float z; };
struct PlayerWorldStateCallbacks {
    void* context;
    bool (*player)(void*, FormKey*);
    std::size_t (*loaded_actor_count)(void*);
    bool (*loaded_actor)(void*, std::size_t, FormKey*);
    bool (*reference_position)(void*, FormKey, Position*);
    bool (*actor_value)(void*, FormKey, const char*, float*);
    bool (*actor_is_dead)(void*, FormKey, bool*);
    std::uint32_t (*runtime_form_id)(void*, FormKey);
};
class PlayerWorldState {
public:
    virtual ~PlayerWorldState() = default;
    virtual std::optional<FormKey> player() = 0;
    virtual std::vector<FormKey> loaded_actors() const = 0;
    virtual std::optional<Position> reference_position(FormKey form) = 0;
    virtual std::optional<float> actor_value(FormKey form, const char* name) = 0;
    virtual std::optional<bool> actor_is_dead(FormKey form) = 0;
    virtual std::uint32_t runtime_form_id(FormKey form) = 0;
};
class CallbackPlayerWorldState final : public PlayerWorldState {
public:
    explicit CallbackPlayerWorldState(const PlayerWorldStateCallbacks& callbacks) : callbacks_(callbacks) {}
    std::optional<FormKey> player() override {
        FormKey value{};
        return callbacks_.player(callbacks_.context, &value) ? std::optional<FormKey>{value} : std::nullopt;
    }
    std::vector<FormKey> loaded_actors() const override {
        std::vector<FormKey> values;
        const auto count = callbacks_.loaded_actor_count(callbacks_.context);
        values.reserve(count);
        for (std::size_t index = 0; index < count; ++index) {
            FormKey value{};
            if (callbacks_.loaded_actor(callbacks_.context, index, &value)) values.push_back(value);
        }
        return values;
    }
    std::optional<Position> reference_position(FormKey form) override {
        Position value{};
        return callbacks_.reference_position(callbacks_.context, form, &value) ? std::optional<Position>{value} : std::nullopt;
    }
    std::optional<float> actor_value(FormKey form, const char* name) override {
        float value{};
        return callbacks_.actor_value(callbacks_.context, form, name, &value) ? std::optional<float>{value} : std::nullopt;
    }
    std::optional<bool> actor_is_dead(FormKey form) override {
        bool value{};
        return callbacks_.actor_is_dead(callbacks_.context, form, &value) ? std::optional<bool>{value} : std::nullopt;
    }
    std::uint32_t runtime_form_id(FormKey form) override {
        return callbacks_.runtime_form_id(callbacks_.context, form);
    }
private:
    const PlayerWorldStateCallbacks& callbacks_;
};
class PlayerCharacter {
public:
    explicit PlayerCharacter(PlayerWorldState& world) : world_(world) {}
    std::optional<FormKey> form_key() { return world_.player(); }
    std::optional<ObjectRefHandle> handle() {
        const auto id = world_.player();
        if (!id) return std::nullopt;
        const auto runtime_id = world_.runtime_form_id(*id);
        return runtime_id != 0 ? std::optional<ObjectRefHandle>{ObjectRefHandle{runtime_id}} : std::nullopt;
    }
    std::optional<Position> position() {
        const auto id = world_.player();
        return id ? world_.reference_position(*id) : std::nullopt;
    }
    std::optional<float> actor_value(const char* name) {
        const auto id = world_.player();
        return id ? world_.actor_value(*id, name) : std::nullopt;
    }
private:
    PlayerWorldState& world_;
};
class ProcessLists {
public:
    explicit ProcessLists(const PlayerWorldState& world) : world_(world) {}
    std::vector<FormKey> actor_forms() const { return world_.loaded_actors(); }
private:
    const PlayerWorldState& world_;
};
class Projectile {
public:
    explicit Projectile(ProjectileHandle handle) : handle_(handle) {}
    UnsupportedHook runtime() const noexcept { return UnsupportedHook::ProjectileRuntime; }
    ProjectileHandle handle() const noexcept { return handle_; }
private:
    ProjectileHandle handle_;
};
struct IFormFactory {
    std::string name;
    FormType form_type;
    FormType GetFormType() const noexcept { return form_type; }
    const char* GetFormName() const noexcept { return name.c_str(); }
    UnsupportedHook Create() const noexcept { return UnsupportedHook::FormCreation; }
};
struct InventoryEntryData {
    FormId object;
    std::int32_t count_delta;
    std::vector<std::uint8_t> extra_types;
    bool worn;
};
struct InventoryChanges {
    FormId owner;
    std::vector<InventoryEntryData> entries;
    std::vector<std::pair<FormId, std::int32_t>> totals;
    std::int32_t GetItemCount(FormId object) const {
        for (const auto& total : totals) if (total.first.value == object.value) return total.second;
        return 0;
    }
    UnsupportedHook AddEntryData(const InventoryEntryData&) const noexcept { return UnsupportedHook::InventoryMutation; }
};
struct ItemList {
    std::vector<InventoryEntryData> items;
    UnsupportedHook GetSelectedItem() const noexcept { return UnsupportedHook::ItemSelection; }
    UnsupportedHook Update() const noexcept { return UnsupportedHook::ItemListUpdate; }
};
enum class ReferenceKind { Actor, Reference };
struct LookupReferenceByHandleResult { FormId form_id; ReferenceKind kind; };
class ReferenceResolver {
public:
    virtual ~ReferenceResolver() = default;
    virtual bool lookup_reference_by_handle(ObjectRefHandle handle,
                                            LookupReferenceByHandleResult& result) const = 0;
};
inline bool LookupReferenceByHandle(const ReferenceResolver& resolver, ObjectRefHandle handle,
                                    LookupReferenceByHandleResult& result) {
    return resolver.lookup_reference_by_handle(handle, result);
}
enum class SIT_SLEEP_STATE : std::uint32_t {
    kNormal = 0, kWantToSit = 1, kWaitingForSitAnim = 2, kIsSitting = 3,
    kRidingMount = kIsSitting,
    kWantToStand = 4, kWantToSleep = 5, kWaitingForSleepAnim = 6,
    kIsSleeping = 7, kWantToWake = 8
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
struct RuntimeSettingRecord { std::uint32_t kind; std::int64_t integer; double float_value; char string_value[256]; };
struct RuntimeReanimateRecord {
    std::uint64_t id; std::uint32_t spell, effect, target; float magnitude;
    double elapsed; bool has_duration; double duration;
};
struct RuntimeErrorRecord { char name[64]; char reason[256]; };
struct RuntimeStateCallbacks {
    bool (*setting)(void*, const char*, RuntimeSettingRecord*);
    float (*sky_hour)(void*);
    bool (*reanimate_effect)(void*, std::uint64_t, RuntimeReanimateRecord*);
    void (*sit_sleep_state)(void*, std::uint32_t, RuntimeErrorRecord*);
    void (*hook)(void*, const char*, RuntimeErrorRecord*);
};
class CallbackTES final : public TES {
public:
    CallbackTES(void* context, const RuntimeStateCallbacks& callbacks) : context_(context), callbacks_(callbacks) {}
    std::optional<Setting> setting(const std::string& name) const override {
        RuntimeSettingRecord record{};
        if (!callbacks_.setting(context_, name.c_str(), &record)) return std::nullopt;
        SettingValue value{};
        switch (record.kind) {
        case 0: value.kind = SettingValue::Kind::Bool; value.boolean_value = record.integer != 0; break;
        case 1: value.kind = SettingValue::Kind::Int; value.int_value = static_cast<std::int32_t>(record.integer); break;
        case 2: value.kind = SettingValue::Kind::Float; value.float_value = static_cast<float>(record.float_value); break;
        case 3: value.kind = SettingValue::Kind::String; value.string_value = record.string_value; break;
        default: return std::nullopt;
        }
        return Setting{name, value};
    }
    Sky sky() const override { return {callbacks_.sky_hour(context_)}; }
    std::optional<ReanimateEffect> reanimate_effect(std::uint64_t id) const override {
        RuntimeReanimateRecord record{};
        if (!callbacks_.reanimate_effect(context_, id, &record)) return std::nullopt;
        return ReanimateEffect{record.id, {record.spell}, {record.effect}, {record.target},
            record.magnitude, record.elapsed, record.has_duration ? std::optional<double>{record.duration} : std::nullopt};
    }
    bool sit_sleep_state(FormId actor, SIT_SLEEP_STATE& state, UnsupportedWorldHook& error) const override {
        (void)state;
        callbacks_.sit_sleep_state(context_, actor.value, &error_);
        error = {error_.name, error_.reason};
        return false;
    }
    UnsupportedWorldHook hook(const std::string& name) const override {
        callbacks_.hook(context_, name.c_str(), &error_);
        return {error_.name, error_.reason};
    }
private:
    void* context_;
    const RuntimeStateCallbacks& callbacks_;
    mutable RuntimeErrorRecord error_{};
};
} // namespace RE
