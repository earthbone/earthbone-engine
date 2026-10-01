// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <utility>
#include <vector>

namespace RE {
enum class NotifyControl { Continue, Stop };

struct EventFormId { std::uint64_t value; };
struct ActorKill { struct Event { EventFormId killer; EventFormId victim; }; };
struct CriticalHit {
    struct Event { EventFormId aggressor; EventFormId weapon; bool sneakHit; };
};

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

enum class ScriptEvent {
    CellFullyLoaded, ContainerChanged, Death, EnterBleedout, FormDelete,
    Furniture, GrabRelease, Harvested, Hit, ObjectLoaded, QuestStage,
    QuestStartStop, Disarmed, MenuOpenClose, CrosshairRef, ActorKill,
    CriticalHit, DragonSoulsGained, LevelIncrease, LocationDiscovery,
    ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned
};
} // namespace RE
