// Generated from crates/commonlib-shim/src/headers.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>

namespace SKSE {
inline constexpr std::uint32_t SOE_PLUGIN_ABI_VERSION = 1;

struct SoePluginManifest {
    std::uint32_t abi_version;
    const char* name;
    const char* const* unsupported_hooks;
    std::size_t unsupported_hook_count;
};

using SoePluginLoad = bool (*)();
} // namespace SKSE

#define SOE_PLUGIN_MANIFEST extern "C" const SKSE::SoePluginManifest SOEPlugin_Manifest
#define SOE_PLUGIN_LOAD extern "C" bool SOEPlugin_Load()
