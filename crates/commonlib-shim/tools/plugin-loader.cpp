// SPDX-License-Identifier: MIT

#include <SKSE/Plugin.h>
#include <cstring>
#include <dlfcn.h>
#include <iostream>
#include <string_view>

int main(int argc, char** argv)
{
    if (argc != 2) {
        std::cerr << "usage: plugin-loader <shared-library>\n";
        return 2;
    }

    void* library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!library) {
        std::cerr << "load error: " << dlerror() << '\n';
        return 1;
    }

    const auto query = reinterpret_cast<bool (*)(SKSE::QueryInterface*, SKSE::PluginInfo*)>(
        dlsym(library, "SKSEPlugin_Query"));
    const auto load = reinterpret_cast<bool (*)(const SKSE::LoadInterface*)>(
        dlsym(library, "SKSEPlugin_Load"));
    const auto* version = static_cast<const SKSE::PluginVersionData*>(
        dlsym(library, "SKSEPlugin_Version"));
    const auto* unsupported = static_cast<const SKSE::UnsupportedHooks*>(
        dlsym(library, "SKSEPlugin_UnsupportedHooks"));
    if (!query || !load || !version) {
        std::cerr << "load error: missing SKSEPlugin_Query, SKSEPlugin_Load or SKSEPlugin_Version\n";
        dlclose(library);
        return 1;
    }
    if (version->dataVersion != SKSE::PluginVersionData::kVersion ||
        version->pluginName[0] == '\0' ||
        !std::memchr(version->pluginName, '\0', sizeof(version->pluginName))) {
        std::cerr << "load error: invalid SKSE plugin version data\n";
        dlclose(library);
        return 1;
    }

    SKSE::QueryInterface queryInterface{
        0x01000000U,
        0,
        0,
        [](std::uint32_t) -> void* { return nullptr; },
    };
    SKSE::PluginInfo info{};
    if (!query(&queryInterface, &info)) {
        std::cerr << "load error: plugin query rejected this runtime\n";
        dlclose(library);
        return 1;
    }
    if (info.infoVersion != SKSE::PluginInfo::kVersion || !info.name ||
        std::string_view(info.name).empty()) {
        std::cerr << "load error: plugin query returned invalid plugin info\n";
        dlclose(library);
        return 1;
    }

    std::cout << "plugin: " << info.name << " v" << info.version << '\n';
    if (!load(&queryInterface)) {
        std::cerr << "load error: plugin entry point returned false\n";
        dlclose(library);
        return 1;
    }
    if (unsupported) {
        if (unsupported->count != 0 && !unsupported->names) {
            std::cerr << "load error: invalid unsupported hook list\n";
            dlclose(library);
            return 1;
        }
        for (std::uint32_t i = 0; i < unsupported->count; ++i) {
            if (!unsupported->names[i] || unsupported->names[i][0] == '\0') {
                std::cerr << "load error: unnamed unsupported hook\n";
                dlclose(library);
                return 1;
            }
            std::cout << "unsupported hook: " << unsupported->names[i] << '\n';
        }
    }
    std::cout << "loaded: " << info.name << '\n';
    dlclose(library);
    return 0;
}
