// Generated from crates/commonlib-shim/src/headers.rs. Do not edit.
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
