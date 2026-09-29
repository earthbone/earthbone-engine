// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace RE {
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

struct UnsupportedHook { const char* name; };
struct HookResult { bool installed; UnsupportedHook unsupported; };
inline HookResult install_hook(UnsupportedHook hook) noexcept { return {false, hook}; }

enum class ScriptEvent {
    CellFullyLoaded, ContainerChanged, Death, EnterBleedout, FormDelete,
    Furniture, GrabRelease, Harvested, Hit, ObjectLoaded, QuestStage,
    QuestStartStop, Disarmed, MenuOpenClose, CrosshairRef, ActorKill,
    CriticalHit, DragonSoulsGained, LevelIncrease, LocationDiscovery,
    ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned
};
} // namespace RE
