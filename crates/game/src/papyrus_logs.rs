// SPDX-License-Identifier: GPL-3.0-or-later

//! Profile-scoped Papyrus logs using the CK runtime error stack layout and the
//! timestamp envelope recognized by the game validation harness.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct FileLogs {
    root: PathBuf,
    trace_enabled: bool,
    debug_information: bool,
    rotated_users: BTreeSet<String>,
}

impl FileLogs {
    pub(super) fn open(
        root: &Path,
        trace_enabled: bool,
        debug_information: bool,
    ) -> io::Result<Self> {
        let script_dir = root.join("Logs/Script");
        fs::create_dir_all(script_dir.join("User"))?;
        rotate(&script_dir.join("Papyrus"), "log")?;
        Ok(Self {
            root: script_dir,
            trace_enabled,
            debug_information,
            rotated_users: BTreeSet::new(),
        })
    }

    pub(super) fn write_vm(&self, message: &str) -> io::Result<()> {
        let is_error = message.starts_with("error:") || message.starts_with("warning:");
        if !is_error && !self.trace_enabled {
            return Ok(());
        }
        let mut output = if is_error {
            self.format_error(message)
        } else if let Some(frame) = message.strip_prefix("  ") {
            self.format_line(&format_stack_frame(frame, self.debug_information))
        } else {
            self.format_line(message)
        };
        if !output.ends_with('\n') {
            output.push('\n');
        }
        append(&self.root.join("Papyrus.0.log"), &output)
    }

    pub(super) fn write_user(&mut self, name: &str, message: &str) -> io::Result<()> {
        let safe_name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "-_ ".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if safe_name.is_empty() {
            return Ok(());
        }
        let directory = self.root.join("User");
        let stem = directory.join(safe_name);
        let key = stem.to_string_lossy().into_owned();
        if self.rotated_users.insert(key) {
            rotate(&stem, "log")?;
        }
        append(
            &directory.join(format!(
                "{}.0.log",
                stem.file_name().unwrap_or_default().to_string_lossy()
            )),
            &format!("{}\n", self.format_line(message)),
        )
    }

    fn format_line(&self, message: &str) -> String {
        format!("[{}] {}", timestamp(), message)
    }

    fn format_error(&self, message: &str) -> String {
        let (body, location) = message
            .rfind(" (")
            .and_then(|open| {
                message
                    .ends_with(')')
                    .then_some((&message[..open], &message[open + 2..message.len() - 1]))
            })
            .unwrap_or((message, ""));
        let mut output = format!("[{}] {body}\nstack:", timestamp());
        if !location.is_empty() {
            output.push_str("\n\t");
            output.push_str(&format_stack_frame(location, self.debug_information));
        }
        output.push('\n');
        output
    }
}

pub(super) fn rotate(stem: &Path, extension: &str) -> io::Result<()> {
    let Some(parent) = stem.parent() else {
        return Ok(());
    };
    let base = stem.file_name().unwrap_or_default().to_string_lossy();
    let mut numbered = Vec::new();
    for entry in fs::read_dir(parent)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(index) = name
            .strip_prefix(&format!("{base}."))
            .and_then(|tail| tail.strip_suffix(&format!(".{extension}")))
            .and_then(|index| index.parse::<u32>().ok())
        else {
            continue;
        };
        numbered.push((index, path));
    }
    numbered.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
    for (index, path) in numbered {
        let next = index.saturating_add(1);
        fs::rename(path, parent.join(format!("{base}.{next}.{extension}")))?;
    }
    Ok(())
}

fn append(path: &Path, text: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(text.as_bytes())
}

fn format_stack_frame(frame: &str, debug_information: bool) -> String {
    let (location, line) = frame
        .rsplit_once(" line ")
        .filter(|(_, line)| line.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or((frame, "?".to_owned()), |(location, line)| {
            (
                location,
                if debug_information {
                    line.to_owned()
                } else {
                    "?".to_owned()
                },
            )
        });
    let (location, line) = location
        .split_once(" instruction ")
        .filter(|(_, instruction)| instruction.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or((location, line), |(location, _)| (location, "?".to_owned()));
    let (script, function) = location.rsplit_once('.').unwrap_or(("", location));
    let script_name = if script.is_empty() {
        "<native>"
    } else {
        script
    };
    format!("[ ].{script_name}.{function}() - \"{script_name}.psc\" Line {line}")
}

fn timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3600;
    let ampm = if hour < 12 { "AM" } else { "PM" };
    let hour = match hour % 12 {
        0 => 12,
        hour => hour,
    };
    format!(
        "{month:02}/{day:02}/{year:04} - {hour:02}:{:02}:{:02}{ampm}",
        day_seconds / 60 % 60,
        day_seconds % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}
