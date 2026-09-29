// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Reference.h"
#include <cstddef>
#include <cstdint>
#include <memory>
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

enum class ScriptEvent {
    CellFullyLoaded, ContainerChanged, Death, EnterBleedout, FormDelete,
    Furniture, GrabRelease, Harvested, Hit, ObjectLoaded, QuestStage,
    QuestStartStop, Disarmed, MenuOpenClose, CrosshairRef, ActorKill,
    CriticalHit, DragonSoulsGained, LevelIncrease, LocationDiscovery,
    ShoutAttack, SkillIncrease, SoulsTrapped, SpellsLearned
};

struct TESGrabReleaseEvent {
    ObjectRefHandle reference;
    bool grabbed;
};
struct TESHarvestedEvent {
    FormId produceItem;
    ActorHandle harvester;
};
struct HitFlags {
    std::uint8_t value{};
    constexpr bool contains(std::uint8_t flag) const noexcept { return (value & flag) == flag; }
};
struct TESHitEvent {
    ObjectRefHandle target;
    ObjectRefHandle cause;
    FormId source;
    FormId projectile;
    HitFlags flags;
};
struct TESObjectLoadedEvent {
    FormId formID;
    bool loaded;
};
struct TESQuestStageEvent {
    FormId formID;
    std::uint16_t stage;
    std::uint8_t itemIndex;
};

enum class HookKind { CallSite, Branch, Vtable };
enum class HookErrorCode { Unsupported };
struct HookError { HookErrorCode code; HookKind kind; };
constexpr HookError request_hook(HookKind kind) noexcept {
    return {HookErrorCode::Unsupported, kind};
}
} // namespace RE
