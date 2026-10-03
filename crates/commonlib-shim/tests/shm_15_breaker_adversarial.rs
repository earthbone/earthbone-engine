// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn compile_and_run(source_text: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::temp_dir().join(format!(
        "soe-shm-15-breaker-{}-{}",
        std::process::id(),
        source_text.len()
    ));
    fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("world.cpp");
    fs::write(&source, source_text).unwrap();
    let binary = scratch.join("world-contract");
    let compiled = Command::new("c++")
        .arg("-std=c++20")
        .arg("-I")
        .arg(root.join("include"))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("a C++20 compiler is required by SHM-15");
    assert!(
        compiled.status.success(),
        "C++ fixture compile failed:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let ran = Command::new(binary).output().unwrap();
    assert!(
        ran.status.success(),
        "C++ fixture failed with {}: {}",
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
}

#[test]
fn callback_tes_preserves_setting_variants_and_present_reanimation_fields() {
    compile_and_run(
        r#"
#include <RE/Skyrim.h>
#include <cstring>
bool setting(void*, const char* name, RE::RuntimeSettingRecord* out) {
    if (std::strcmp(name, "bool") == 0) { out->kind = 0; out->integer = 0; return true; }
    if (std::strcmp(name, "int") == 0) { out->kind = 1; out->integer = -19; return true; }
    if (std::strcmp(name, "float") == 0) { out->kind = 2; out->float_value = 2.5; return true; }
    if (std::strcmp(name, "string") == 0) { out->kind = 3; std::strcpy(out->string_value, "hello"); return true; }
    return false;
}
float sky(void*) { return 18.5F; }
bool reanimate(void*, std::uint64_t id, RE::RuntimeReanimateRecord* out) {
    if (id != 42) return false;
    *out = {42, 1, 2, 3, 4.5F, 6.25, true, 10.0};
    return true;
}
void sit_sleep(void*, std::uint32_t, RE::RuntimeErrorRecord* out) {
    std::strcpy(out->name, "SIT_SLEEP_STATE");
    std::strcpy(out->reason, "unsupported");
}
void hook(void*, const char*, RE::RuntimeErrorRecord* out) {
    std::strcpy(out->name, "binary hook");
    std::strcpy(out->reason, "unsupported");
}
int main() {
    const RE::RuntimeStateCallbacks callbacks{setting, sky, reanimate, sit_sleep, hook};
    const RE::CallbackTES world(nullptr, callbacks);
    const auto b = world.setting("bool");
    const auto i = world.setting("int");
    const auto f = world.setting("float");
    const auto s = world.setting("string");
    if (!b || b->value.boolean_value) return 1;
    if (!i || i->value.int_value != -19) return 2;
    if (!f || f->value.float_value != 2.5F) return 3;
    if (!s || s->value.string_value != "hello") return 4;
    if (world.sky().hour != 18.5F) return 5;
    const auto effect = world.reanimate_effect(42);
    if (!effect || effect->id != 42 || effect->spell.value != 1 || effect->effect.value != 2 ||
        effect->target.value != 3 || effect->magnitude != 4.5F || effect->elapsed != 6.25 ||
        effect->duration != 10.0) return 6;
    if (world.reanimate_effect(43)) return 7;
    return 0;
}
"#,
    );
}

#[test]
fn callback_tes_rejects_integer_setting_outside_commonlib_range() {
    compile_and_run(
        r#"
#include <RE/Skyrim.h>
#include <limits>
bool setting(void*, const char*, RE::RuntimeSettingRecord* out) {
    out->kind = 1;
    out->integer = std::numeric_limits<std::int64_t>::max();
    return true;
}
float sky(void*) { return 0.0F; }
bool reanimate(void*, std::uint64_t, RE::RuntimeReanimateRecord*) { return false; }
void sit_sleep(void*, std::uint32_t, RE::RuntimeErrorRecord*) {}
void hook(void*, const char*, RE::RuntimeErrorRecord*) {}
int main() {
    const RE::RuntimeStateCallbacks callbacks{setting, sky, reanimate, sit_sleep, hook};
    const RE::CallbackTES world(nullptr, callbacks);
    return world.setting("iTest") ? 1 : 0;
}
"#,
    );
}
