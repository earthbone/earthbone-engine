// Generated from crates/commonlib-shim/src/vm.rs. Do not edit.
#pragma once
#include <cstddef>
#include <cstdint>
#include <algorithm>
#include <cctype>
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
    bool register_native(NativeDecl declaration) {
        const auto key = make_key(declaration.class_name, declaration.function);
        const auto found = std::find_if(natives_.begin(), natives_.end(), [&](const auto& item) {
            return make_key(item.class_name, item.function) == key;
        });
        if (found != natives_.end()) {
            duplicates_.push_back(std::move(declaration));
            return false;
        }
        natives_.push_back(std::move(declaration));
        return true;
    }
    const NativeDecl* get(const std::string& class_name, const std::string& function) const {
        const auto key = make_key(class_name, function);
        const auto found = std::find_if(natives_.begin(), natives_.end(), [&](const auto& item) {
            return make_key(item.class_name, item.function) == key;
        });
        return found == natives_.end() ? nullptr : &*found;
    }
    const std::vector<NativeDecl>& duplicates() const noexcept { return duplicates_; }
    const std::vector<NativeDecl>& entries() const noexcept { return natives_; }
    std::size_t len() const noexcept { return natives_.size(); }
    bool is_empty() const noexcept { return natives_.empty(); }
private:
    static std::string make_key(const std::string& class_name, const std::string& function) {
        std::string key = class_name;
        key.push_back('.');
        key += function;
        std::transform(key.begin(), key.end(), key.begin(), [](unsigned char value) {
            return static_cast<char>(std::tolower(value));
        });
        return key;
    }
    std::vector<NativeDecl> natives_;
    std::vector<NativeDecl> duplicates_;
};
} // namespace RE
