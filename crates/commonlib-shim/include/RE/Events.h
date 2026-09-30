// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Reference.h"
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace RE {
struct TESContainerChangedEvent {
    std::optional<FormId> oldContainer;
    std::optional<FormId> newContainer;
    FormId baseObj;
    std::int32_t itemCount;
    ObjectRefHandle reference;
    std::uint16_t uniqueID;
};
struct TESDeathEvent {
    ObjectRefHandle actorDying;
    std::optional<ObjectRefHandle> actorKiller;
    bool dead;
};
struct TESEnterBleedoutEvent { ObjectRefHandle actor; };
struct TESFormDeleteEvent { FormId formID; };
enum class FurnitureEventType : std::uint32_t { Enter = 0, Exit = 1 };
struct TESFurnitureEvent {
    ObjectRefHandle actor;
    ObjectRefHandle targetFurniture;
    FurnitureEventType type;
};

class EventState {
public:
    virtual ~EventState() = default;
    virtual std::optional<std::int32_t> inventory_count(FormId container, FormId item) const = 0;
    virtual std::optional<bool> is_dead(ActorHandle actor) const = 0;
    virtual std::optional<bool> is_in_bleedout(ActorHandle actor) const = 0;
    virtual std::optional<bool> is_form_deleted(FormId form) const = 0;
    virtual std::optional<TESFurnitureEvent> furniture_event(ActorHandle actor) const = 0;
};

struct UnsupportedHook { const char* name; const char* reason; };
inline constexpr UnsupportedHook UNSUPPORTED_VTABLE_HOOK{
    "vtable and call-site hooks target Skyrim machine code, which this engine does not use",
    "vtable and call-site hooks target Skyrim machine code, which this engine does not use"};

enum class NotifyControl { Continue, Stop };

template <class Event>
class EventSink {
public:
    virtual ~EventSink() = default;
    virtual NotifyControl process(const Event& event) = 0;
};

template <class Event>
class EventSource {
public:
    void add_sink(std::unique_ptr<EventSink<Event>> sink) { sinks_.push_back(std::move(sink)); }
    std::size_t send(const Event& event) {
        std::size_t seen = 0;
        for (const auto& sink : sinks_) {
            ++seen;
            if (sink->process(event) == NotifyControl::Stop) break;
        }
        return seen;
    }
private:
    std::vector<std::unique_ptr<EventSink<Event>>> sinks_;
};

struct FormIdentity { std::uint64_t value; };
struct DisarmedEvent { FormIdentity source; FormIdentity target; };
struct DragonSoulsGained { float souls; };
struct LevelIncrease { FormIdentity player; std::uint16_t new_level; };
struct LocationDiscovery { FormIdentity map_marker; std::optional<std::string> worldspace_id; };

class ScriptEventSourceHolder {
public:
    EventSource<DisarmedEvent>& disarmed() noexcept { return disarmed_; }
    EventSource<DragonSoulsGained>& dragon_souls_gained() noexcept { return dragon_souls_gained_; }
    EventSource<LevelIncrease>& level_increase() noexcept { return level_increase_; }
    EventSource<LocationDiscovery>& location_discovery() noexcept { return location_discovery_; }
private:
    EventSource<DisarmedEvent> disarmed_;
    EventSource<DragonSoulsGained> dragon_souls_gained_;
    EventSource<LevelIncrease> level_increase_;
    EventSource<LocationDiscovery> location_discovery_;
};

struct HookResult { bool installed; UnsupportedHook unsupported; };
inline HookResult install_hook(UnsupportedHook hook) noexcept { return {false, hook}; }

enum class ScriptEvent {
    CellFullyLoaded, ContainerChanged, Death, EnterBleedout, FormDelete,
    Furniture, GrabRelease, Harvested, Hit, ObjectLoaded, QuestStage,
    QuestStartStop, Disarmed, MenuOpenClose, CrosshairRef, ActorKill,
    CriticalHit, DragonSoulsGained, LevelIncrease, LocationDiscovery,
    ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned
};

struct ShoutAttack { FormId shout; };
struct SkillIncrease { ActorHandle player; ActorValue actor_value; };
struct SoulsTrapped { ActorHandle trapper; ActorHandle target; };
struct SpellsLearned { FormId spell; };
struct TESCellFullyLoadedEvent { FormId cell; };
} // namespace RE
