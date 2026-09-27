// Generated from crates/commonlib-shim/src/util.rs. Do not edit.
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
