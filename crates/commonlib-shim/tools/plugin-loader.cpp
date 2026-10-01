// SPDX-License-Identifier: MIT

#include <SKSE/Plugin.h>
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

    const auto* manifest = static_cast<const SKSE::SoePluginManifest*>(dlsym(library, "SOEPlugin_Manifest"));
    const auto load = reinterpret_cast<SKSE::SoePluginLoad>(dlsym(library, "SOEPlugin_Load"));
    if (!manifest || !load) {
        std::cerr << "load error: missing SOEPlugin_Manifest or SOEPlugin_Load\n";
        dlclose(library);
        return 1;
    }
    if (manifest->abi_version != SKSE::SOE_PLUGIN_ABI_VERSION || !manifest->name ||
        (manifest->unsupported_hook_count != 0 && !manifest->unsupported_hooks)) {
        std::cerr << "load error: invalid plugin manifest\n";
        dlclose(library);
        return 1;
    }
    for (std::size_t index = 0; index < manifest->unsupported_hook_count; ++index) {
        const char* hook = manifest->unsupported_hooks[index];
        if (!hook || std::string_view(hook).empty()) {
            std::cerr << "load error: unnamed unsupported hook\n";
            dlclose(library);
            return 1;
        }
    }

    std::cout << "plugin: " << manifest->name << '\n';
    for (std::size_t index = 0; index < manifest->unsupported_hook_count; ++index) {
        std::cout << "unsupported hook: " << manifest->unsupported_hooks[index] << '\n';
    }
    if (!load()) {
        std::cerr << "load error: plugin entry point returned false\n";
        dlclose(library);
        return 1;
    }
    std::cout << "loaded: " << manifest->name << '\n';
    dlclose(library);
    return 0;
}
