// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved fragment tasks from FallrimTools FragmentTask.java, Skyrim layout.

use super::{bad, read_count, read_string, Cursor, RefId, Result, StringRef, Value};

/// The task payload associated with an active thread's fragment discriminator.
#[derive(Clone, Debug)]
pub enum FragmentTask {
    Type2(Value),
    Named {
        data: FragmentData,
        variable: Option<Value>,
    },
}

/// Names and payloads retained by the saved Papyrus fragment dispatcher.
#[derive(Clone, Debug)]
pub enum FragmentData {
    QuestStage {
        quest: RefId,
        stage: u16,
        flags: u8,
    },
    ScenePhaseResults {
        scene: RefId,
        value: u32,
    },
    SceneActionResults {
        scene: RefId,
        value: u32,
    },
    SceneResults {
        scene: RefId,
    },
    TerminalRunResults {
        unknown: u8,
        value: u32,
        reference: RefId,
        name: StringRef,
    },
    TopicInfo,
}

impl FragmentTask {
    pub(super) fn read(cursor: &mut Cursor<'_>, strings: usize, kind: u8) -> Result<Self> {
        if kind == 2 {
            return Ok(Self::Type2(Value::read(cursor, strings, 0)?));
        }
        let length = read_count(cursor, 1, "fragment name bytes")?;
        let data = match cursor.take(length)? {
            b"QuestStage" => FragmentData::QuestStage {
                quest: cursor.ref_id()?,
                stage: cursor.u16()?,
                flags: cursor.u8()?,
            },
            b"ScenePhaseResults" => FragmentData::ScenePhaseResults {
                scene: cursor.ref_id()?,
                value: cursor.u32()?,
            },
            b"SceneActionResults" => FragmentData::SceneActionResults {
                scene: cursor.ref_id()?,
                value: cursor.u32()?,
            },
            b"SceneResults" => FragmentData::SceneResults {
                scene: cursor.ref_id()?,
            },
            b"TerminalRunResults" => FragmentData::TerminalRunResults {
                unknown: cursor.u8()?,
                value: cursor.u32()?,
                reference: cursor.ref_id()?,
                name: read_string(cursor, strings)?,
            },
            b"TopicInfo" => FragmentData::TopicInfo,
            name => {
                return Err(bad(format!(
                    "unknown fragment task {}",
                    String::from_utf8_lossy(name)
                )))
            }
        };
        let variable = if kind == 3 {
            Some(Value::read(cursor, strings, 0)?)
        } else {
            None
        };
        Ok(Self::Named { data, variable })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>, kind: u8) -> Result<()> {
        match self {
            Self::Type2(value) if kind == 2 => value.write(out),
            Self::Named { data, variable }
                if matches!(kind, 1 | 3) && variable.is_some() == (kind == 3) =>
            {
                let name: &[u8] = match data {
                    FragmentData::QuestStage { .. } => b"QuestStage",
                    FragmentData::ScenePhaseResults { .. } => b"ScenePhaseResults",
                    FragmentData::SceneActionResults { .. } => b"SceneActionResults",
                    FragmentData::SceneResults { .. } => b"SceneResults",
                    FragmentData::TerminalRunResults { .. } => b"TerminalRunResults",
                    FragmentData::TopicInfo => b"TopicInfo",
                };
                out.extend_from_slice(&(name.len() as u32).to_le_bytes());
                out.extend_from_slice(name);
                match data {
                    FragmentData::QuestStage {
                        quest,
                        stage,
                        flags,
                    } => {
                        quest.write(out);
                        out.extend_from_slice(&stage.to_le_bytes());
                        out.push(*flags);
                    }
                    FragmentData::ScenePhaseResults { scene, value }
                    | FragmentData::SceneActionResults { scene, value } => {
                        scene.write(out);
                        out.extend_from_slice(&value.to_le_bytes());
                    }
                    FragmentData::SceneResults { scene } => scene.write(out),
                    FragmentData::TerminalRunResults {
                        unknown,
                        value,
                        reference,
                        name,
                    } => {
                        out.push(*unknown);
                        out.extend_from_slice(&value.to_le_bytes());
                        reference.write(out);
                        out.extend_from_slice(&name.0.to_le_bytes());
                    }
                    FragmentData::TopicInfo => {}
                }
                if let Some(value) = variable {
                    value.write(out)?;
                }
                Ok(())
            }
            _ => Err(bad("fragment task disagrees with discriminator")),
        }
    }
}
