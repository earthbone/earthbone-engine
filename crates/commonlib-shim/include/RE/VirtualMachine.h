// Generated from crates/commonlib-shim/src/vm.rs. Do not edit.
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
