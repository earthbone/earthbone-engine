// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
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
struct UnsupportedHook { const char* name; };
class WorldBindings {
public:
    virtual ~WorldBindings() = default;
    virtual std::vector<std::uint8_t> save_state() const = 0;
    virtual bool load_state(const std::vector<std::uint8_t>& bytes, std::string& error) = 0;
    virtual std::vector<CALCED_OBJECT> calculate_current_form_list(
        std::uint32_t list, std::uint16_t level, std::int16_t count, std::string& error) = 0;
    virtual UnsupportedHook beam_projectile_impact() const = 0;
    virtual UnsupportedHook character_flags() const = 0;
};
class BGSSaveLoadGame {
public:
    explicit BGSSaveLoadGame(WorldBindings& runtime) : runtime_(runtime) {}
    std::vector<std::uint8_t> save_state() const { return runtime_.save_state(); }
    bool load_state(const std::vector<std::uint8_t>& bytes, std::string& error) {
        return runtime_.load_state(bytes, error);
    }
    UnsupportedHook get_change() const { return {"BGSSaveLoadGame::GetChange"}; }
    UnsupportedHook is_form_id_in_use() const { return {"BGSSaveLoadGame::IsFormIDInUse"}; }
private: WorldBindings& runtime_;
};
class BGSSaveLoadManager : public BGSSaveLoadGame {
public:
    using BGSSaveLoadGame::BGSSaveLoadGame;
    std::vector<std::uint8_t> save() const { return save_state(); }
    bool load(const std::vector<std::uint8_t>& bytes, std::string& error) {
        return load_state(bytes, error);
    }
    UnsupportedHook save_file() const { return {"BGSSaveLoadManager::Save"}; }
    UnsupportedHook load_file() const { return {"BGSSaveLoadManager::Load"}; }
    UnsupportedHook load_most_recent_save_game() const {
        return {"BGSSaveLoadManager::LoadMostRecentSaveGame"};
    }
    UnsupportedHook populate_save_list() const {
        return {"BGSSaveLoadManager::PopulateSaveList"};
    }
};
class BeamProjectile {
public:
    explicit BeamProjectile(WorldBindings& runtime) : runtime_(runtime) {}
    UnsupportedHook add_impact() const { return runtime_.beam_projectile_impact(); }
private: WorldBindings& runtime_;
};
class CharacterControllerFlags {
public:
    explicit CharacterControllerFlags(WorldBindings& runtime) : runtime_(runtime) {}
    UnsupportedHook get() const { return runtime_.character_flags(); }
private: WorldBindings& runtime_;
};
} // namespace RE
