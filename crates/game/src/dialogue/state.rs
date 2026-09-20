// SPDX-License-Identifier: GPL-3.0-or-later

//! SOEW dialogue cursor and said-once/reset history. Definitions and the
//! derived topic index are re-read from the same load order after restoration.

use std::collections::BTreeMap;

use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

use super::{Session, State};
use crate::world::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id};
use crate::World;

impl World {
    pub(crate) fn write_dialogue(&self, w: &mut Writer) {
        w.bool(self.dialogue.session.is_some());
        if let Some(session) = &self.dialogue.session {
            write_form_id(w, session.speaker);
            write_form_id(w, session.listener);
            write_opt_form_id(w, session.topic);
            write_opt_form_id(w, session.current_info);
            write_opt_form_id(w, session.last_info);
            w.u32(session.response_index);
        }
        w.len(self.dialogue.said.len());
        for (id, time) in &self.dialogue.said {
            write_form_id(w, *id);
            w.f64(*time);
        }
    }

    pub(crate) fn validate_dialogue(&self, state: &State) -> Result<(), VmError> {
        let invalid = || VmError::State("invalid dialogue cursor".into());
        if let Some(session) = &state.session {
            if session.speaker == session.listener {
                return Err(invalid());
            }
            if let Some(info) = session.current_info {
                let topic = self.dialogue_topic_of(info).ok_or_else(invalid)?;
                if session.topic != Some(topic) {
                    return Err(invalid());
                }
                let (_, data) = self.response_data(info).map_err(|_| invalid())?;
                if session.response_index as usize >= data.responses.len() {
                    return Err(invalid());
                }
            } else if session.response_index != 0 {
                return Err(invalid());
            }
            if let Some(info) = session.last_info {
                self.dialogue_info(info).map_err(|_| invalid())?;
            }
        }
        Ok(())
    }
}

pub(crate) fn read_dialogue(r: &mut Reader<'_>) -> Result<State, VmError> {
    let session = if r.bool()? {
        Some(Session {
            speaker: read_form_id(r)?,
            listener: read_form_id(r)?,
            topic: read_opt_form_id(r)?,
            current_info: read_opt_form_id(r)?,
            last_info: read_opt_form_id(r)?,
            response_index: r.u32()?,
        })
    } else {
        None
    };
    let count = r.len()?;
    let mut said = BTreeMap::new();
    for _ in 0..count {
        let id = read_form_id(r)?;
        let time = r.f64()?;
        if !time.is_finite() || said.insert(id, time).is_some() {
            return Err(VmError::State("invalid dialogue history".into()));
        }
    }
    Ok(State { session, said })
}
