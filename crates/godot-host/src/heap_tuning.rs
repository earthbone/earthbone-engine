// SPDX-License-Identifier: GPL-3.0-or-later

//! glibc streaming allocation controls, from the local malloc.h and mallopt(3).
//!
//! The streaming workers allocate and free large cell payloads on many threads.
//! With glibc's default of eight times the core count in arenas, each worker
//! thread ends up with its own arena, and freed payloads in one arena are not
//! reused by the others. The full LOD visibility test measured 17.7 to 18.1 GiB
//! of RSS at the default and 12.8 to 12.9 GiB with the arena count capped at 2,
//! three runs each, with the same live Rust heap of about 4 GiB. Capping the
//! arena count returns the difference without changing what the host holds.

/// Arena count used when neither the environment nor the caller asks for another.
pub const DEFAULT_ARENA_CAP: i32 = 2;

/// Keep streaming buffers at or above 128 KiB eligible for direct mappings.
pub const DEFAULT_MMAP_THRESHOLD: i32 = 128 * 1024;

/// glibc `M_ARENA_MAX` from `<malloc.h>`.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const M_ARENA_MAX: i32 = -8;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const M_MMAP_THRESHOLD: i32 = -3;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
extern "C" {
    fn mallopt(param: i32, value: i32) -> i32;
    fn mallinfo2() -> HeapInfo;
    fn malloc_trim(pad: usize) -> i32;
}

/// glibc heap totals. Mapped allocations are separate from arena allocations.
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
pub struct HeapInfo {
    pub arena: usize,
    pub ordblks: usize,
    pub smblks: usize,
    pub hblks: usize,
    pub hblkhd: usize,
    pub usmblks: usize,
    pub fsmblks: usize,
    pub uordblks: usize,
    pub fordblks: usize,
    pub keepcost: usize,
}

/// Snapshot libc ownership without forcing collection or changing RSS.
pub fn heap_info() -> Option<HeapInfo> {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallinfo2 returns a value and synchronizes its arena traversal.
        Some(unsafe { mallinfo2() })
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    None
}

/// Return unused arena pages after a large location's preparation owners drop.
/// Live allocations keep their contents and addresses. Other libc hosts skip it.
pub fn release_unused_pages() -> bool {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: malloc_trim synchronizes arenas and only releases unused pages.
        unsafe { malloc_trim(0) != 0 }
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    false
}

/// Whether an allocator setting was applied or left to the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapSetting {
    /// The requested value was applied.
    Applied(i32),
    /// An explicit allocator environment setting keeps priority.
    EnvironmentWins,
    /// This libc has no arena control.
    Unsupported,
}

/// Cap the number of malloc arenas the process may create from now on.
///
/// Arenas that already exist stay. Call this before the host starts worker
/// threads. A `MALLOC_ARENA_MAX` environment variable takes priority so a run
/// can still compare other values.
pub fn cap_malloc_arenas(max: i32) -> HeapSetting {
    if environment_controls("MALLOC_ARENA_MAX", "glibc.malloc.arena_max") {
        return HeapSetting::EnvironmentWins;
    }
    apply(max)
}

/// Prevent temporary cell buffers from raising glibc's adaptive mmap threshold.
/// See the local mallopt(3) description of M_MMAP_THRESHOLD. Explicit allocator
/// environment settings take priority, including GLIBC_TUNABLES.
pub fn map_large_buffers() -> HeapSetting {
    if environment_controls("MALLOC_MMAP_THRESHOLD_", "glibc.malloc.mmap_threshold") {
        return HeapSetting::EnvironmentWins;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: mallopt changes the synchronized process allocator setting.
        if unsafe { mallopt(M_MMAP_THRESHOLD, DEFAULT_MMAP_THRESHOLD) } == 1 {
            HeapSetting::Applied(DEFAULT_MMAP_THRESHOLD)
        } else {
            HeapSetting::Unsupported
        }
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    HeapSetting::Unsupported
}

fn environment_controls(variable: &str, tunable: &str) -> bool {
    std::env::var_os(variable).is_some()
        || std::env::var("GLIBC_TUNABLES").is_ok_and(|value| {
            value
                .split(':')
                .any(|entry| entry.split_once('=').is_some_and(|(key, _)| key == tunable))
        })
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn apply(max: i32) -> HeapSetting {
    // SAFETY: mallopt only reads two integers and updates glibc's global
    // tunable under its own lock.
    let accepted = unsafe { mallopt(M_ARENA_MAX, max) };
    if accepted == 1 {
        HeapSetting::Applied(max)
    } else {
        HeapSetting::Unsupported
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn apply(_max: i32) -> HeapSetting {
    HeapSetting::Unsupported
}

#[cfg(all(test, target_os = "linux", target_env = "gnu"))]
mod tests {
    use super::*;
    use std::ffi::{c_char, c_void};

    extern "C" {
        fn open_memstream(buffer: *mut *mut c_char, size: *mut usize) -> *mut c_void;
        fn malloc_info(options: i32, stream: *mut c_void) -> i32;
        fn fclose(stream: *mut c_void) -> i32;
        fn free(pointer: *mut c_void);
    }

    const CHILD_MARKER: &str = "SOE_HEAP_TUNING_CHILD";

    fn rss_kib() -> usize {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse().ok())
            })
            .unwrap()
    }

    #[test]
    fn mapped_temporary_buffers_release_without_trimming_live_anchors() {
        if let Ok(mode) = std::env::var(CHILD_MARKER) {
            assert_eq!(cap_malloc_arenas(2), HeapSetting::Applied(2));
            // SAFETY: The child simulates a raised threshold before its fixture.
            assert_eq!(unsafe { mallopt(M_MMAP_THRESHOLD, 16 * 1024 * 1024) }, 1);
            if mode == "mapped" {
                assert_eq!(
                    map_large_buffers(),
                    HeapSetting::Applied(DEFAULT_MMAP_THRESHOLD)
                );
            }
            let mut blocks = Vec::new();
            let mut anchors = Vec::new();
            for _ in 0..160 {
                blocks.push(vec![1_u8; 1024 * 1024]);
                anchors.push(vec![2_u8; 4096]);
            }
            let before = rss_kib();
            drop(blocks);
            let returned = before.saturating_sub(rss_kib());
            if mode == "mapped" {
                assert!(
                    returned >= 128 * 1024,
                    "mapped buffers returned only {returned} KiB"
                );
            } else {
                assert!(
                    returned < 32 * 1024,
                    "fixture did not retain arena buffers: {returned} KiB"
                );
            }
            assert!(anchors.iter().flatten().all(|byte| *byte == 2));
            return;
        }
        for mode in ["arena", "mapped"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "heap_tuning::tests::mapped_temporary_buffers_release_without_trimming_live_anchors",
                    "--test-threads=1",
                ])
                .env(CHILD_MARKER, mode)
                .env_remove("MALLOC_ARENA_MAX")
                .env_remove("MALLOC_MMAP_THRESHOLD_")
                .env_remove("GLIBC_TUNABLES")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{mode}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn mmap_threshold_environment_keeps_priority() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            assert_eq!(map_large_buffers(), HeapSetting::EnvironmentWins);
            return;
        }
        for (variable, value) in [
            ("MALLOC_MMAP_THRESHOLD_", "1048576"),
            ("GLIBC_TUNABLES", "glibc.malloc.mmap_threshold=1048576"),
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "heap_tuning::tests::mmap_threshold_environment_keeps_priority",
                    "--test-threads=1",
                ])
                .env(CHILD_MARKER, "1")
                .env_remove("MALLOC_MMAP_THRESHOLD_")
                .env_remove("GLIBC_TUNABLES")
                .env(variable, value)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{variable}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    /// Number of arenas glibc reports, read from `malloc_info`.
    fn arena_count() -> usize {
        let mut buffer: *mut c_char = std::ptr::null_mut();
        let mut size = 0usize;
        // SAFETY: open_memstream owns the buffer until fclose, and the slice is
        // read before free releases it.
        unsafe {
            let stream = open_memstream(&raw mut buffer, &raw mut size);
            assert!(!stream.is_null());
            assert_eq!(malloc_info(0, stream), 0);
            assert_eq!(fclose(stream), 0);
            let text =
                String::from_utf8_lossy(std::slice::from_raw_parts(buffer.cast::<u8>(), size))
                    .into_owned();
            free(buffer.cast());
            text.matches("<heap nr=").count()
        }
    }

    fn allocate_on_many_threads() {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    let blocks: Vec<Vec<u8>> = (0..64).map(|_| vec![1u8; 256 * 1024]).collect();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    blocks.len()
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap_or(0), 64);
        }
    }

    /// Runs in a child process because arenas that exist cannot be removed, and
    /// the lib test binary has other threads that already created some.
    #[test]
    fn prf_20_arena_cap_limits_arenas_created_by_many_threads() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            assert_eq!(cap_malloc_arenas(2), HeapSetting::Applied(2));
            allocate_on_many_threads();
            let arenas = arena_count();
            assert!(arenas <= 2, "arena count {arenas} exceeds the cap of 2");
            return;
        }
        let executable = std::env::current_exe().expect("test binary path");
        let output = std::process::Command::new(executable)
            .args([
                "--exact",
                "heap_tuning::tests::prf_20_arena_cap_limits_arenas_created_by_many_threads",
                "--test-threads=1",
            ])
            .env(CHILD_MARKER, "1")
            .env_remove("MALLOC_ARENA_MAX")
            .env_remove("GLIBC_TUNABLES")
            .output()
            .expect("spawn child test");
        assert!(
            output.status.success(),
            "child failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The same child without the cap shows the cap is what bounds the count.
    #[test]
    fn prf_20_without_the_cap_many_threads_create_more_arenas() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            allocate_on_many_threads();
            let arenas = arena_count();
            assert!(arenas > 2, "expected more than 2 arenas, saw {arenas}");
            return;
        }
        let executable = std::env::current_exe().expect("test binary path");
        let output = std::process::Command::new(executable)
            .args([
                "--exact",
                "heap_tuning::tests::prf_20_without_the_cap_many_threads_create_more_arenas",
                "--test-threads=1",
            ])
            .env(CHILD_MARKER, "1")
            .env_remove("MALLOC_ARENA_MAX")
            .env_remove("GLIBC_TUNABLES")
            .output()
            .expect("spawn child test");
        assert!(
            output.status.success(),
            "child failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn prf_20_environment_setting_keeps_priority() {
        if environment_controls("MALLOC_ARENA_MAX", "glibc.malloc.arena_max") {
            assert_eq!(cap_malloc_arenas(2), HeapSetting::EnvironmentWins);
        } else {
            assert_ne!(cap_malloc_arenas(64), HeapSetting::EnvironmentWins);
        }
    }

    #[test]
    fn released_payload_pages_return_without_dropping_live_anchors() {
        if std::env::var_os(CHILD_MARKER).is_some() {
            assert_eq!(cap_malloc_arenas(2), HeapSetting::Applied(2));
            // SAFETY: This child sets glibc thresholds before allocating its fixture.
            // Keep large blocks in arenas and suppress automatic top-chunk trimming.
            unsafe {
                assert_eq!(mallopt(-3, 16 * 1024 * 1024), 1);
                assert_eq!(mallopt(-1, 32 * 1024 * 1024), 1);
            }
            let mut blocks = Vec::new();
            let mut anchors = Vec::new();
            for _ in 0..192 {
                blocks.push(vec![1_u8; 1024 * 1024]);
                anchors.push(vec![2_u8; 4096]);
            }
            drop(blocks);
            let rss = || {
                std::fs::read_to_string("/proc/self/status")
                    .unwrap()
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("VmRSS:")
                            .and_then(|value| value.split_whitespace().next())
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap()
            };
            let before = rss();
            assert!(release_unused_pages());
            let after = rss();
            assert!(
                before.saturating_sub(after) >= 128 * 1024,
                "at least 128 MiB must return to the OS, before={before} KiB after={after} KiB"
            );
            assert!(anchors.iter().flatten().all(|byte| *byte == 2));
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "heap_tuning::tests::released_payload_pages_return_without_dropping_live_anchors",
                "--test-threads=1",
            ])
            .env(CHILD_MARKER, "1")
            .env_remove("MALLOC_ARENA_MAX")
            .env_remove("GLIBC_TUNABLES")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
