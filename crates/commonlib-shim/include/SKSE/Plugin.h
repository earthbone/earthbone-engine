// Generated from crates/commonlib-shim/src/headers.rs. Do not edit.
#pragma once
#include <cstdint>

namespace SKSE {
class QueryInterface;
class LoadInterface;

struct PluginInfo {
    static constexpr std::uint32_t kVersion = 1;
    std::uint32_t infoVersion;
    const char* name;
    std::uint32_t version;
};

// CommonLibSSE-NG PluginVersionData prefix, read by the SKSE loader.
struct PluginVersionData {
    static constexpr std::uint32_t kVersion = 1;
    std::uint32_t dataVersion{kVersion};
    std::uint32_t pluginVersion{};
    char pluginName[256]{};
};
} // namespace SKSE

#define SKSE_PLUGIN_QUERY(...) extern "C" bool SKSEPlugin_Query(__VA_ARGS__)
#define SKSE_PLUGIN_LOAD(...) extern "C" bool SKSEPlugin_Load(__VA_ARGS__)
#define SKSE_PLUGIN_VERSION extern "C" const SKSE::PluginVersionData SKSEPlugin_Version
