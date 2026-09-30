// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include <array>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>
namespace RE {
struct ActorValue {
    std::uint32_t value;
    static const ActorValue HEALTH;
    static const ActorValue MAGICKA;
    static const ActorValue STAMINA;
};
inline const ActorValue ActorValue::HEALTH{24};
inline const ActorValue ActorValue::MAGICKA{25};
inline const ActorValue ActorValue::STAMINA{26};
enum class UnsupportedHook { ArrowProjectile, LoadFormBuffer, ActivateReferenceChildren };

class ActorValueWorldState {
public:
    virtual ~ActorValueWorldState() = default;
    virtual std::optional<float> actor_value(std::uint32_t actor, ActorValue value) const = 0;
    virtual bool set_actor_value(std::uint32_t actor, ActorValue value, float amount) = 0;
    virtual std::optional<std::string> actor_value_name(ActorValue value) const = 0;
    virtual std::optional<ActorValue> actor_value_by_name(const std::string& name) const = 0;
    virtual std::optional<FormId> default_object(std::array<char, 4> key) const = 0;
};

class ActorValueList {
public:
    explicit ActorValueList(ActorValueWorldState& state) : state_(state) {}
    std::optional<float> get_actor_value_info(std::uint32_t actor, ActorValue value) const {
        return state_.actor_value(actor, value);
    }
    bool set_actor_value(std::uint32_t actor, ActorValue value, float amount) {
        return state_.set_actor_value(actor, value, amount);
    }
    std::optional<std::string> get_actor_value_info(ActorValue value) const {
        return state_.actor_value_name(value);
    }
    std::optional<std::string> get_actor_value_name(ActorValue value) const {
        return state_.actor_value_name(value);
    }
    std::optional<ActorValue> lookup_actor_value_by_name(const std::string& name) const {
        return state_.actor_value_by_name(name);
    }
private:
    ActorValueWorldState& state_;
};

class BGSDefaultObjectManager {
public:
    explicit BGSDefaultObjectManager(const ActorValueWorldState& state) : state_(state) {}
    std::optional<FormId> get(std::array<char, 4> key) const { return state_.default_object(key); }
private:
    const ActorValueWorldState& state_;
};

struct ArrowProjectile {
    static constexpr UnsupportedHook save_game() noexcept { return UnsupportedHook::ArrowProjectile; }
    static constexpr UnsupportedHook load_game() noexcept { return UnsupportedHook::ArrowProjectile; }
    static constexpr UnsupportedHook process_impacts() noexcept { return UnsupportedHook::ArrowProjectile; }
};
struct BGSLoadFormBuffer {
    static constexpr UnsupportedHook get_version() noexcept { return UnsupportedHook::LoadFormBuffer; }
    static constexpr UnsupportedHook load_form() noexcept { return UnsupportedHook::LoadFormBuffer; }
};
enum class ACTOR_CRITICAL_STAGE : std::uint32_t { kNone = 0, kGooStart = 1, kGooEnd = 2, kDisintegrateStart = 3, kDisintegrateEnd = 4 };
enum class ACTOR_VALUE_MODIFIER : std::uint32_t { kPermanent = 0, kTemporary = 1, kDamage = 2 };
struct ActivateParentID { FormId refID; float activateDelay; };
struct AIProcess { std::optional<FormId> runningPackage; std::string procedure; };
struct ActiveEffect {
    std::uint64_t id;
    FormId spell;
    FormId effect;
    FormId source;
    std::optional<FormId> caster;
    FormId target;
    float magnitude;
    std::optional<double> duration;
    double elapsed;
};
struct UnsupportedState { const char* message; };
class WorldState {
public:
    virtual ~WorldState() = default;
    virtual ACTOR_CRITICAL_STAGE critical_stage(FormId actor) = 0;
    virtual bool set_critical_stage(FormId actor, ACTOR_CRITICAL_STAGE stage) = 0;
    virtual float actor_value_modifier(FormId actor, std::uint32_t actorValue, ACTOR_VALUE_MODIFIER modifier) = 0;
    virtual AIProcess ai_process(FormId actor) = 0;
    virtual std::vector<ActivateParentID> activate_parents(FormId reference) = 0;
    virtual std::optional<ActiveEffect> active_effect(std::uint64_t id) const = 0;
    virtual UnsupportedState unsupported_hook(const std::string& name) const = 0;
};
} // namespace RE

#include <cstdint>
#include <string>
#include <vector>

namespace RE {
struct ContainerItemExtra {
    std::uint32_t owner{};
    std::uint32_t owner_rank{};
    std::uint32_t owner_global{};
    std::uint32_t health_bits{};
    bool has_owner{};
    bool has_owner_rank{};
    bool has_owner_global{};
    bool has_health{};
};
struct CALCED_OBJECT { std::uint32_t form_id; std::uint16_t count; ContainerItemExtra container_item; };
enum class CHARACTER_FLAGS : std::uint32_t {
    kNone = 0, kQuadruped = 1u << 0, kNoGravityOnGround = 1u << 1,
    kTryStep = 1u << 2, kNoFriction = 1u << 3, kAllowJumpNoContact = 1u << 4,
    kStuckQuad = 1u << 5, kAnimAngleMod = 1u << 6, kHitDamage = 1u << 7,
    kHitFlags = 1u << 7, kSupport = 1u << 8, kHasPotentialSupportManifold = 1u << 9,
    kCanJump = 1u << 10, kChaseBip = 1u << 11, kFollowRagdoll = 1u << 12,
    kJumping = 1u << 13, kNotPushable = 1u << 14, kFloatLand = 1u << 15,
    kCheckSupport = 1u << 16, kNoSim = 1u << 17, kFarAway = 1u << 18,
    kOnStilts = 1u << 19, kQuickSimulate = 1u << 20, kRecordHits = 1u << 21,
    kComputeTiltPreIntegrate = 1u << 22, kShouldersUnderWater = 1u << 23,
    kOnStairs = 1u << 24, kCanPitch = 1u << 25, kCanRoll = 1u << 26,
    kNoCharacterCollisions = 1u << 27, kNotPushablePermanent = 1u << 28,
    kPossiblePathObstacle = 1u << 29, kShapeRequiresZRot = 1u << 30,
    kSwimAtWaterSurface = 1u << 31
};
struct NamedUnsupportedHook { const char* name; };
class WorldBindings {
public:
    virtual ~WorldBindings() = default;
    virtual std::vector<std::uint8_t> save_state() const = 0;
    virtual bool load_state(const std::vector<std::uint8_t>& bytes, std::string& error) = 0;
    virtual std::vector<CALCED_OBJECT> calculate_current_form_list(
        std::uint32_t list, std::uint16_t level, std::int16_t count, std::string& error) = 0;
    virtual NamedUnsupportedHook beam_projectile_impact() const = 0;
    virtual NamedUnsupportedHook character_flags() const = 0;
};
class BGSSaveLoadGame {
public:
    explicit BGSSaveLoadGame(WorldBindings& runtime) : runtime_(runtime) {}
    std::vector<std::uint8_t> save_state() const { return runtime_.save_state(); }
    bool load_state(const std::vector<std::uint8_t>& bytes, std::string& error) {
        return runtime_.load_state(bytes, error);
    }
    NamedUnsupportedHook get_change() const { return {"BGSSaveLoadGame::GetChange"}; }
    NamedUnsupportedHook is_form_id_in_use() const { return {"BGSSaveLoadGame::IsFormIDInUse"}; }
private: WorldBindings& runtime_;
};
class BGSSaveLoadManager : public BGSSaveLoadGame {
public:
    using BGSSaveLoadGame::BGSSaveLoadGame;
    std::vector<std::uint8_t> save() const { return save_state(); }
    bool load(const std::vector<std::uint8_t>& bytes, std::string& error) {
        return load_state(bytes, error);
    }
    NamedUnsupportedHook save_file() const { return {"BGSSaveLoadManager::Save"}; }
    NamedUnsupportedHook load_file() const { return {"BGSSaveLoadManager::Load"}; }
    NamedUnsupportedHook load_most_recent_save_game() const {
        return {"BGSSaveLoadManager::LoadMostRecentSaveGame"};
    }
    NamedUnsupportedHook populate_save_list() const {
        return {"BGSSaveLoadManager::PopulateSaveList"};
    }
};
class BeamProjectile {
public:
    explicit BeamProjectile(WorldBindings& runtime) : runtime_(runtime) {}
    NamedUnsupportedHook add_impact() const { return runtime_.beam_projectile_impact(); }
private: WorldBindings& runtime_;
};
class CharacterControllerFlags {
public:
    explicit CharacterControllerFlags(WorldBindings& runtime) : runtime_(runtime) {}
    NamedUnsupportedHook get() const { return runtime_.character_flags(); }
private: WorldBindings& runtime_;
};
} // namespace RE

namespace RE {
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
