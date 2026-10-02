// SPDX-License-Identifier: MIT

//! Generated C++ declarations for the Rust shim surface.
//!
//! The declarations are kept here beside the Rust API they project. Run
//! `cargo run -p soe-commonlib-shim --bin generate-headers` after changing
//! this map. The RE and SKSE interface declarations are a source-level API.
//! The plugin entry point has a C ABI but no bridge to a running engine.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const RE_FORM: &str = r"// Generated from crates/commonlib-shim/src/form.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
struct FormId {
    std::uint32_t value;
    friend constexpr bool operator==(FormId, FormId) = default;
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
";

const RE_REFERENCE: &str = r#"// Generated from crates/commonlib-shim/src/refr.rs. Do not edit.
#pragma once
#include "RE/Math.h"
#include "RE/Form.h"
#include "RE/World.h"
#include "RE/WorldTypes.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
struct ObjectRefHandle { std::uint32_t value; };
struct ActorHandle {
    std::uint32_t value;
    constexpr operator ObjectRefHandle() const noexcept { return {value}; }
};
struct ActorPtr { std::optional<ActorHandle> handle; };
struct ActorState {
    bool dead{};
    bool unconscious{};
    bool sneaking{};
    bool weapon_drawn{};
    bool blocking{};
};
enum class ActorHookError { AddressPatch, CallDetour, VtableOverride, Trampoline };
enum class ActorApiError { InvalidActor, UnknownForm, UnsupportedOperation };

struct ExtraLeveledItemBase { FormId leveled_item; };
struct ExtraLightData {
    float fov;
    float fade;
    float end_distance_cap;
    float shadow_depth_bias;
    std::uint8_t unknown;
};
struct LinkedRefChild { FormId keyword; ObjectRefHandle reference; };
struct ExtraMapMarker { std::string name; std::uint8_t flags; std::uint16_t marker_type; };
struct MissingLinkedRefId { FormId keyword; FormId linked_reference; };

class ReferenceExtraData {
public:
    virtual ~ReferenceExtraData() = default;
    virtual std::optional<ExtraLeveledItemBase> leveled_item_base() const = 0;
    virtual std::optional<ExtraLightData> light_data() const = 0;
    virtual const std::vector<LinkedRefChild>& linked_ref_children() const = 0;
    virtual std::optional<ExtraMapMarker> map_marker() const = 0;
    virtual const std::vector<MissingLinkedRefId>& missing_linked_ref_ids() const = 0;
};
struct ReferenceUnsupportedHookInfo { const char* name; const char* reason; };
inline constexpr ReferenceUnsupportedHookInfo EXTRA_DATA_HOOKS{
    "CommonLib extra-data vtable hooks",
    "Skyrim executable vtables have no target in the Rust engine"};
enum class WEAPON_STATE : std::uint32_t {
    kSheathed = 0, kWantToDraw = 1, kDrawing = 2,
    kDrawn = 3, kWantToSheathe = 4, kSheathing = 5
};
struct Unsupported {
    const char* operation;
    std::optional<UnsupportedHook> hook;
    Unsupported(const char* operation) : operation(operation), hook(std::nullopt) {}
    Unsupported(UnsupportedHook hook) : operation(nullptr), hook(hook) {}
};
struct TESObjectREFRPtr {
    std::optional<ObjectRefHandle> handle;
    bool is_null() const noexcept { return !handle.has_value(); }
};

class ObjectRef {
public:
    virtual ~ObjectRef() = default;
    virtual ObjectRefHandle handle() const = 0;
    virtual FormId form_id() const = 0;
    virtual FormId base() const = 0;
    virtual NiPoint3 position() const = 0;
    virtual NiPoint3 rotation() const = 0;
    virtual std::optional<FormId> parent_cell() const = 0;
    virtual bool is_disabled() const = 0;
    virtual bool is_deleted() const = 0;
    virtual Unsupported set_base(FormId base) = 0;
};
using TESObjectREFR = ObjectRef;

class Actor : public ObjectRef {
public:
    virtual ActorHandle actor_handle() const = 0;
    virtual ActorPtr actor_ptr() const = 0;
    virtual ActorState state() const = 0;
    virtual std::optional<float> actor_value(ActorValue av) const = 0;
    virtual std::optional<ActorApiError> set_actor_value(ActorValue av, float value) = 0;
    virtual FormId race() const = 0;
    virtual bool is_dead() const = 0;
    virtual bool is_player() const = 0;
    virtual WEAPON_STATE weapon_state() const = 0;
    virtual bool add_spell(FormId spell) = 0;
    virtual bool add_perk(FormId perk) = 0;
    virtual bool add_item(FormId item, std::uint32_t count) = 0;
};

class ActorEquipManager {
public:
    virtual ~ActorEquipManager() = default;
    virtual std::optional<ActorApiError> equip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<ActorApiError> unequip_object(ActorHandle actor, FormId object) = 0;
    virtual std::optional<ActorHookError> reject_hook(ActorHookError hook) const = 0;
};
} // namespace RE
"#;

const RE_EXTRA_DATA: &str = r#"// Generated from crates/commonlib-shim/src/extra.rs and extra_data.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Math.h"
#include "RE/Reference.h"
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

namespace RE {
enum class ExtraDataType : std::uint8_t { LeveledCreature = 0x2D, Hotkey = 0x49, Enchantment = 0x9B };
struct ExtraEnchantment { FormId enchantment; std::uint16_t charge; std::optional<bool> remove_on_unequip; friend bool operator==(const ExtraEnchantment&, const ExtraEnchantment&) = default; };
struct ExtraHotkey { std::optional<std::uint8_t> slot; friend bool operator==(const ExtraHotkey&, const ExtraHotkey&) = default; };
struct ExtraLeveledCreature { FormId original_base; FormId template_base; friend bool operator==(const ExtraLeveledCreature&, const ExtraLeveledCreature&) = default; };

class ExtraDataSource {
public:
    virtual ~ExtraDataSource() = default;
    virtual bool has_type(ExtraDataType kind) const = 0;
    virtual std::optional<ExtraEnchantment> enchantment() const = 0;
    virtual std::optional<ExtraHotkey> hotkey() const = 0;
    virtual std::optional<ExtraLeveledCreature> leveled_creature() const = 0;
};
class ExtraDataList {
public:
    explicit ExtraDataList(const ExtraDataSource& source) : source_(source) {}
    bool has_type(ExtraDataType kind) const { return source_.has_type(kind); }
    std::optional<ExtraEnchantment> enchantment() const { return source_.enchantment(); }
    std::optional<ExtraHotkey> hotkey() const { return source_.hotkey(); }
    std::optional<ExtraLeveledCreature> leveled_creature() const { return source_.leveled_creature(); }
    static constexpr const char* ADD_UNSUPPORTED = "ExtraDataList::Add mutates engine-owned list state";
    static constexpr const char* REMOVE_UNSUPPORTED = "ExtraDataList::Remove mutates engine-owned list state";
    static constexpr const char* REMOVE_BY_TYPE_UNSUPPORTED = "ExtraDataList::RemoveByType mutates engine-owned list state";
    static constexpr const char* SET_ENCHANTMENT_UNSUPPORTED = "ExtraDataList::SetEnchantment mutates engine-owned list state";
private:
    const ExtraDataSource& source_;
};

struct DoorTeleportData {
    ObjectRefHandle linkedDoor;
    NiPoint3 position;
    NiPoint3 rotation;
    std::int8_t flags;
};

enum class ReferenceExtraKind { MissingRefIDs, NorthRotation, OutfitItem, Poison, Teleport };
struct ReferenceExtraValue {
    ReferenceExtraKind kind;
    std::vector<ActivateParentID> missingRefIDs;
    std::optional<float> northRotation;
    std::optional<FormId> outfitItem;
    std::optional<FormId> poison;
    std::uint32_t poisonCount{};
    std::optional<DoorTeleportData> teleport;
};

class ReferenceExtraQuery {
public:
    virtual ~ReferenceExtraQuery() = default;
    virtual bool supports_extra(ReferenceExtraKind kind) const = 0;
    virtual std::optional<ReferenceExtraValue> extra(ObjectRefHandle reference, ReferenceExtraKind kind) const = 0;
};

class HookRegistry {
public:
    static bool reject(const std::string& name, std::string& error) {
        error = "unsupported binary hook: " + name;
        return false;
    }
};
} // namespace RE
"#;

const RE_MAGIC: &str = r#"// Generated from the SHM-13 MIT runtime contract. Do not edit.
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
"#;

const RE_MATH: &str = r"// Generated from crates/commonlib-shim/src/ni.rs. Do not edit.
#pragma once
#include <cstdint>
#include <optional>
#include <string>

namespace RE {
struct NiPoint3 {
    float x{};
    float y{};
    float z{};
    constexpr NiPoint3() = default;
    constexpr NiPoint3(float x_value, float y_value, float z_value) : x(x_value), y(y_value), z(z_value) {}
    float length() const;
    float dot(NiPoint3 other) const;
    constexpr NiPoint3 operator-(NiPoint3 other) const {
        return {x - other.x, y - other.y, z - other.z};
    }
};

struct NiMatrix3 {
    float value[3][3];
    NiMatrix3() : value{{1, 0, 0}, {0, 1, 0}, {0, 0, 1}} {}
    NiPoint3 apply(NiPoint3 point) const;
};

struct NiTransform {
    NiMatrix3 rotate{};
    NiPoint3 translate{};
    float scale{1.0F};
    NiPoint3 apply(NiPoint3 point) const;
};

struct SceneNode { std::uint64_t value; };
class SceneGraph {
public:
    virtual ~SceneGraph() = default;
    virtual std::optional<SceneNode> find_by_name(SceneNode root, const std::string& name) const = 0;
    virtual NiTransform world_transform(SceneNode node) const = 0;
    virtual void set_local_transform(SceneNode node, NiTransform transform) = 0;
};
} // namespace RE
";

const RE_WORLD: &str = r#"// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/WorldTypes.h"
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
template <typename T>
struct Result { std::optional<T> value; std::optional<UnsupportedState> error; };
class WorldState {
public:
    virtual ~WorldState() = default;
    virtual Result<ACTOR_CRITICAL_STAGE> critical_stage(FormId actor) = 0;
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
"#;

const RE_WORLD_TYPES: &str = r#"// Generated from crates/commonlib-shim/src/world_types.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Math.h"
#include <optional>
#include <string>

namespace RE {
enum class UnsupportedHook {
    FormCreation, InventoryMutation, ItemSelection, ItemListUpdate,
    BinaryAddress, VtablePatch, UnsupportedEquipOption,
    ProjectileRuntime, BinaryPatch, ArrowProjectile, LoadFormBuffer, ActivateReferenceChildren
};
struct SettingValue {
    enum class Kind { Bool, Int, Float, String };
    Kind kind;
    bool boolean_value{};
    std::int32_t int_value{};
    float float_value{};
    std::string string_value;
};
struct WornState { bool worn; bool worn_left; };
struct HazardState { FormId reference; FormId base; NiPoint3 position; bool enabled; };
struct HighProcessState {
    FormId actor;
    bool in_combat;
    std::optional<FormId> combat_target;
    bool staggered;
};
struct HitData {
    FormId attacker;
    FormId target;
    FormId weapon;
    float weapon_damage;
    float armor_reduction;
    float health_damage;
    bool blocked;
    bool killed;
};
struct UnsupportedHookInfo { std::string type_name; std::string hook_name; std::string reason; };

class GameSettingCollection {
public:
    virtual ~GameSettingCollection() = default;
    virtual std::optional<SettingValue> get_setting(const std::string& name) const = 0;
};
class WorldTypes {
public:
    virtual ~WorldTypes() = default;
    virtual std::optional<WornState> extra_worn(FormId actor, FormId item) const = 0;
    virtual std::optional<HazardState> hazard(FormId reference) const = 0;
    virtual std::optional<HighProcessState> high_process(FormId actor) const = 0;
    virtual std::optional<HitData> resolve_hit(FormId attacker, FormId target, FormId weapon) = 0;
    virtual UnsupportedHookInfo unsupported_hook(const std::string& type_name, const std::string& hook_name) const = 0;
};
} // namespace RE
"#;

const RE_WORLD_PLAYER: &str = r#"// Generated from crates/commonlib-shim/src/world.rs. Do not edit.
#pragma once
#include "RE/Reference.h"
#include "RE/WorldTypes.h"
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace RE {
struct FormKey { std::uint16_t file; std::uint32_t object; };
struct ProjectileHandle { std::uint32_t value; };
struct Position { float x; float y; float z; };
class PlayerWorldState {
public:
    virtual ~PlayerWorldState() = default;
    virtual std::optional<FormKey> player() = 0;
    virtual std::vector<FormKey> loaded_actors() const = 0;
    virtual std::optional<Position> reference_position(FormKey form) = 0;
    virtual std::optional<float> actor_value(FormKey form, const char* name) = 0;
    virtual std::optional<bool> actor_is_dead(FormKey form) = 0;
};
class PlayerCharacter {
public:
    explicit PlayerCharacter(PlayerWorldState& world) : world_(world) {}
    std::optional<FormKey> form_key() { return world_.player(); }
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
} // namespace RE
"#;

const RE_EVENTS: &str = r#"// Generated from crates/commonlib-shim/src/events.rs. Do not edit.
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

struct UnsupportedEventHook { const char* name; const char* reason; };
inline constexpr UnsupportedEventHook UNSUPPORTED_VTABLE_HOOK{
    "vtable and call-site hooks target Skyrim machine code, which this engine does not use",
    "vtable and call-site hooks target Skyrim machine code, which this engine does not use"};
using FormID = std::uint32_t;

struct TESQuestStartStopEvent {
    FormID formID;
    bool started;
    bool failed;
    std::uint16_t pad06;
};
static_assert(sizeof(TESQuestStartStopEvent) == 0x8);

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

struct HookResult { bool installed; UnsupportedEventHook unsupported; };
inline HookResult install_hook(UnsupportedEventHook hook) noexcept { return {false, hook}; }

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
"#;

const RE_UTILITY: &str = r"// Generated from crates/commonlib-shim/src/util.rs. Do not edit.
#pragma once
#include <memory>
#include <shared_mutex>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace RE {
class FixedString {
public:
    const std::string& as_str() const noexcept { return *value_; }
    friend bool operator==(const FixedString& left, const FixedString& right) noexcept {
        return left.value_ == right.value_;
    }
private:
    explicit FixedString(std::shared_ptr<const std::string> value) : value_(std::move(value)) {}
    std::shared_ptr<const std::string> value_;
    friend class StringCache;
};

class StringCache {
public:
    FixedString intern(const std::string& value);
private:
    std::shared_mutex mutex_;
    std::unordered_map<std::string, std::shared_ptr<const std::string>> table_;
};

template <class T> using Array = std::vector<T>;
template <class T> using ReadWriteLock = std::shared_mutex;
} // namespace RE
";

const RE_VM: &str = r"// Generated from crates/commonlib-shim/src/vm.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <tuple>
#include <vector>

namespace RE {
struct StaticFunctionTag {};
struct VmHandle { std::uint64_t value; };
struct VmStackId { std::uint32_t value; };
struct NativeDecl { std::string class_name; std::string function; bool latent; };

class NativeRegistry {
public:
    // Rust names this method `register`, which is a C++ keyword.
    bool register_native(NativeDecl declaration);
    const NativeDecl* get(const std::string& class_name, const std::string& function) const;
    const std::vector<NativeDecl>& duplicates() const noexcept;
    std::size_t len() const noexcept;
    bool is_empty() const noexcept;
private:
    std::vector<NativeDecl> natives_;
    std::vector<NativeDecl> duplicates_;
};
} // namespace RE
";

const SKSE_INTERFACES: &str = r#"// Generated from crates/commonlib-shim/src/skse.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/VirtualMachine.h"
#include <array>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace SKSE {
struct PluginHandle {
    std::uint32_t value;
    static const PluginHandle INVALID;
};
inline const PluginHandle PluginHandle::INVALID{0xFFFFFFFF};
struct PluginDeclaration { std::string name; std::uint32_t version; std::string author; };
enum class MessageKind { PostLoad, PostPostLoad, DataLoaded, NewGame, PreLoadGame, PostLoadGame, SaveGame, DeleteGame };

class MessagingInterface {
public:
    virtual ~MessagingInterface() = default;
    virtual void register_listener(PluginHandle plugin, std::function<void(MessageKind)> listener) = 0;
    virtual void dispatch(MessageKind kind) = 0;
};
class SerializationInterface {
public:
    virtual ~SerializationInterface() = default;
    virtual void set_unique_id(PluginHandle plugin, std::array<std::uint8_t, 4> id) = 0;
    virtual bool write_record(std::array<std::uint8_t, 4> kind, std::uint32_t version,
                              const std::vector<std::uint8_t>& data) = 0;
    virtual std::optional<std::tuple<std::array<std::uint8_t, 4>, std::uint32_t,
                                     std::vector<std::uint8_t>>> read_record() = 0;
    virtual std::optional<RE::FormId> resolve_form_id(RE::FormId saved) const = 0;
};
class TaskInterface {
public:
    virtual ~TaskInterface() = default;
    virtual void add_task(std::function<void()> task) = 0;
    virtual void add_ui_task(std::function<void()> task) = 0;
};
class PapyrusInterface {
public:
    virtual ~PapyrusInterface() = default;
    virtual bool register_natives(std::function<bool(RE::NativeRegistry&)> callback) = 0;
};
struct Unsupported { const char* what; };
inline constexpr Unsupported TRAMPOLINE{
    "SKSE::Trampoline patches machine code and has no target in this engine"};
} // namespace SKSE
"#;

const SKSE_PLUGIN: &str = r#"// Generated from crates/commonlib-shim/src/headers.rs. Do not edit.
#pragma once
#include <algorithm>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <iterator>

namespace SKSE {
struct QueryInterface {
    std::uint32_t runtimeVersion;
    std::uint32_t editorVersion;
    std::uint32_t isEditor;
    void* (*queryInterface)(std::uint32_t);
};

using LoadInterface = QueryInterface;

struct PluginInfo {
    static constexpr std::uint32_t kVersion = 1;
    std::uint32_t infoVersion;
    const char* name;
    std::uint32_t version;
};

// CommonLibSSE-NG PluginVersionData layout, read by the SKSE loader.
struct PluginVersionData {
    static constexpr std::uint32_t kVersion = 1;
    std::uint32_t dataVersion{kVersion};
    std::uint32_t pluginVersion{};
    char pluginName[256]{};
    char author[256]{};
    char supportEmail[252]{};
    bool noStructUse : 1 {};
    std::uint8_t padding1 : 7 {};
    std::uint8_t padding2{};
    std::uint16_t padding3{};
    bool addressLibrary : 1 {};
    bool sigScanning : 1 {};
    bool structsPost629 : 1 {};
    std::uint8_t padding4 : 5 {};
    std::uint8_t padding5{};
    std::uint16_t padding6{};
    std::uint32_t compatibleVersions[16]{};
    std::uint32_t xseMinimum{};

    PluginVersionData& PluginVersion(std::uint32_t value) noexcept {
        pluginVersion = value;
        return *this;
    }
    PluginVersionData& PluginName(const char* value) noexcept {
        copy_string(pluginName, value);
        return *this;
    }
    PluginVersionData& AuthorName(const char* value) noexcept {
        copy_string(author, value);
        return *this;
    }
    PluginVersionData& SupportEmail(const char* value) noexcept {
        copy_string(supportEmail, value);
        return *this;
    }
    PluginVersionData& AuthorEmail(const char* value) noexcept {
        copy_string(supportEmail, value);
        return *this;
    }
    PluginVersionData& CompatibleVersions(std::initializer_list<std::uint32_t> values) noexcept {
        std::fill(std::begin(compatibleVersions), std::end(compatibleVersions), 0);
        std::copy_n(values.begin(), std::min(values.size(), std::size(compatibleVersions)),
                    compatibleVersions);
        return *this;
    }
    PluginVersionData& MinimumSKSEVersion(std::uint32_t value) noexcept {
        xseMinimum = value;
        return *this;
    }
    PluginVersionData& MinimumRequiredXSEVersion(std::uint32_t value) noexcept {
        xseMinimum = value;
        return *this;
    }
    PluginVersionData& UsesAddressLibrary(bool value = true) noexcept {
        addressLibrary = value;
        return *this;
    }
    PluginVersionData& UsesSigScanning(bool value = true) noexcept {
        sigScanning = value;
        return *this;
    }
    PluginVersionData& UsesStructsPost629(bool value = true) noexcept {
        structsPost629 = value;
        return *this;
    }

private:
    template <std::size_t N>
    static void copy_string(char (&target)[N], const char* value) noexcept {
        std::fill(std::begin(target), std::end(target), '\0');
        if (value) {
            const auto length = std::min(std::strlen(value), N - 1);
            std::memcpy(target, value, length);
        }
    }
};

struct UnsupportedHooks {
    const char* const* names;
    std::uint32_t count;
};
} // namespace SKSE

#define SKSE_PLUGIN_QUERY(...) extern "C" bool SKSEPlugin_Query(__VA_ARGS__)
#define SKSE_PLUGIN_LOAD(...) extern "C" bool SKSEPlugin_Load(__VA_ARGS__)
#define SKSE_PLUGIN_VERSION extern "C" const SKSE::PluginVersionData SKSEPlugin_Version
#define SKSE_PLUGIN_UNSUPPORTED_HOOKS extern "C" const SKSE::UnsupportedHooks SKSEPlugin_UnsupportedHooks
"#;

const RE_CONDITIONS: &str = r#"// Generated from crates/commonlib-shim/src/conditions.rs. Do not edit.
#pragma once
#include "RE/Form.h"
#include "RE/Reference.h"
#include <cstddef>
#include <cstdint>
#include <vector>

namespace RE {
enum class CONDITIONITEMOBJECT : std::uint8_t {
    kSelf = 0, kTarget = 1, kRef = 2, kCombatTarget = 3, kLinkedRef = 4,
    kQuestAlias = 5, kPackData = 6, kEventData = 7, kCommandTarget = 8
};
enum class ConditionErrorCode : std::uint8_t {
    None, UnsupportedFunction, UnsupportedTarget, UnsupportedHook, UnavailableState, InvalidCondition
};
enum class ConditionHook : std::uint8_t { Copy };
struct ConditionError { ConditionErrorCode code; std::uint32_t detail; };
struct ComparisonValue {
    bool is_global;
    float number;
    FormId global;
};
struct ConditionContext {
    FormId owner;
    ObjectRefHandle subject;
    ObjectRefHandle target;
    bool has_target;
    FormId quest;
    bool has_quest;
};
struct CONDITION_ITEM_DATA {
    std::uint8_t flags;
    ComparisonValue comparison;
    std::uint16_t function;
    std::uint32_t parameters[2];
    std::uint32_t run_on;
    FormId reference;
    std::int32_t parameter_3;
    CONDITIONITEMOBJECT object;
};
class ConditionRuntime {
public:
    virtual ~ConditionRuntime() = default;
    virtual ConditionError evaluate_item(const CONDITION_ITEM_DATA& item,
                                         const ConditionContext& context,
                                         bool& result) = 0;
};
struct TESConditionItem {
    CONDITION_ITEM_DATA data;
    ConditionError IsTrue(ConditionRuntime& runtime, const ConditionContext& context,
                          bool& result) const {
        return runtime.evaluate_item(data, context, result);
    }
    ConditionError operator()(ConditionRuntime& runtime, const ConditionContext& context,
                              bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError is_true(ConditionRuntime& runtime, const ConditionContext& context,
                           bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError Copy() const { return copy(); }
    ConditionError copy() const {
        return {ConditionErrorCode::UnsupportedHook,
                static_cast<std::uint32_t>(ConditionHook::Copy)};
    }
};
struct TESCondition {
    std::vector<TESConditionItem> items;
    ConditionError IsTrue(ConditionRuntime& runtime, const ConditionContext& context,
                          bool& result) const {
        return is_true(runtime, context, result);
    }
    ConditionError operator()(ConditionRuntime& runtime, const ConditionContext& context,
                              bool& result) const {
        return IsTrue(runtime, context, result);
    }
    ConditionError is_true(ConditionRuntime& runtime, const ConditionContext& context,
                           bool& result) const {
        bool any = false;
        for (std::size_t index = 0; index < items.size(); ++index) {
            const auto& item = items[index];
            if (!any) {
                bool value = false;
                auto error = item.is_true(runtime, context, value);
                if (error.code != ConditionErrorCode::None) return error;
                any = value;
            }
            if ((item.data.flags & 1U) == 0 || index + 1 == items.size()) {
                if (!any) { result = false; return {}; }
                any = false;
            }
        }
        result = true;
        return {};
    }
    ConditionError Copy() const { return copy(); }
    ConditionError copy() const {
        return {ConditionErrorCode::UnsupportedHook,
                static_cast<std::uint32_t>(ConditionHook::Copy)};
    }
};
} // namespace RE
"#;

const FILES: &[(&str, &str)] = &[
    ("RE/Form.h", RE_FORM),
    ("RE/Reference.h", RE_REFERENCE),
    ("RE/World.h", RE_WORLD),
    ("RE/WorldPlayer.h", RE_WORLD_PLAYER),
    ("RE/Math.h", RE_MATH),
    ("RE/ExtraData.h", RE_EXTRA_DATA),
    ("RE/Events.h", RE_EVENTS),
    ("RE/Utility.h", RE_UTILITY),
    ("RE/VirtualMachine.h", RE_VM),
    ("RE/Magic.h", RE_MAGIC),
    ("RE/WorldTypes.h", RE_WORLD_TYPES),
    ("RE/Conditions.h", RE_CONDITIONS),
    ("RE/Skyrim.h", "// Generated umbrella for the supported RE surface. Do not edit.\n#pragma once\n#include \"RE/Form.h\"\n#include \"RE/Reference.h\"\n#include \"RE/Math.h\"\n#include \"RE/World.h\"\n#include \"RE/WorldPlayer.h\"\n#include \"RE/ExtraData.h\"\n#include \"RE/Magic.h\"\n#include \"RE/WorldTypes.h\"\n#include \"RE/Conditions.h\"\n#include \"RE/Events.h\"\n#include \"RE/Utility.h\"\n#include \"RE/VirtualMachine.h\"\n"),
    ("SKSE/Interfaces.h", SKSE_INTERFACES),
    ("SKSE/Plugin.h", SKSE_PLUGIN),
    ("SKSE/SKSE.h", "// Generated umbrella for the supported SKSE surface. Do not edit.\n#pragma once\n#include \"SKSE/Interfaces.h\"\n#include \"SKSE/Plugin.h\"\n"),
];

/// Write every generated header below `root` and return its relative paths.
pub fn write_headers(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut written = Vec::with_capacity(FILES.len());
    for (relative, contents) in FILES {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        written.push(PathBuf::from(relative));
    }
    Ok(written)
}

/// Paths and contents in stable order, used by the generation check.
pub fn generated_headers() -> &'static [(&'static str, &'static str)] {
    FILES
}
