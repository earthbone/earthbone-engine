// SPDX-License-Identifier: GPL-3.0-or-later

//! INFO ownership follows TopicChildren groups, as in Mutagen's DialogTopic
//! reader. PNAM ordering adapts Wrye Bash _DialChildren._sort_group (GPL):
//! stable siblings, orphan records retained, and explicit cycle rejection.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use soe_esm::{Entry, GlobalFormId, Group, GroupKind, LoadOrder, RecordRef, Tag};

#[derive(Debug, Default)]
pub(crate) struct Index {
    pub parents: BTreeMap<GlobalFormId, GlobalFormId>,
    pub topics: BTreeMap<GlobalFormId, Result<Vec<GlobalFormId>, String>>,
}

impl Index {
    pub fn build(order: &LoadOrder) -> Self {
        let mut parents = BTreeMap::new();
        let mut sequence = Vec::new();
        let mut seen = BTreeSet::new();
        for (file, loaded) in order.files().iter().enumerate() {
            for group in &loaded.plugin.groups {
                visit(
                    order,
                    file,
                    group,
                    None,
                    &mut parents,
                    &mut sequence,
                    &mut seen,
                );
            }
        }
        let mut topics: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for info in sequence {
            if let Some(topic) = parents.get(&info) {
                topics.entry(*topic).or_default().push(info);
            }
        }
        let topics = topics
            .into_iter()
            .map(|(topic, infos)| (topic, ordered(order, infos)))
            .collect();
        Self { parents, topics }
    }
}

fn visit(
    order: &LoadOrder,
    file: usize,
    group: &Group,
    parent: Option<GlobalFormId>,
    parents: &mut BTreeMap<GlobalFormId, GlobalFormId>,
    sequence: &mut Vec<GlobalFormId>,
    seen: &mut BTreeSet<GlobalFormId>,
) {
    let parent = match group.kind {
        GroupKind::TopicChildren(raw) => Some(order.resolve(file, raw)),
        _ => parent,
    };
    for entry in &group.entries {
        match entry {
            Entry::Group(group) => visit(order, file, group, parent, parents, sequence, seen),
            Entry::Record(index) => {
                let record = &order.file(file).plugin.records[*index];
                if record.tag != Tag(*b"INFO") {
                    continue;
                }
                let id = order.resolve(file, record.form_id);
                if seen.insert(id) {
                    sequence.push(id);
                }
                if order.winner_ref(id)
                    == Some(RecordRef {
                        file,
                        record: *index,
                    })
                {
                    if let Some(parent) = parent {
                        parents.insert(id, parent);
                    }
                }
            }
        }
    }
}

fn ordered(order: &LoadOrder, infos: Vec<GlobalFormId>) -> Result<Vec<GlobalFormId>, String> {
    let all: BTreeSet<_> = infos.iter().copied().collect();
    let mut sorted = Vec::new();
    let mut remaining = VecDeque::new();
    for id in infos {
        let previous = order.winner_ref(id).and_then(|rref| {
            let data = order.record(rref).find(Tag(*b"PNAM"))?;
            let raw = u32::from_le_bytes(data.try_into().ok()?);
            (raw != 0).then(|| order.resolve(rref.file, raw))
        });
        if let Some(previous) = previous.filter(|id| all.contains(id)) {
            remaining.push_back((id, previous));
        } else {
            sorted.push(id);
        }
    }
    while !remaining.is_empty() {
        let before = remaining.len();
        for _ in 0..before {
            let Some((id, previous)) = remaining.pop_back() else {
                break;
            };
            if let Some(index) = sorted.iter().position(|id| *id == previous) {
                sorted.insert(index + 1, id);
            } else {
                remaining.push_front((id, previous));
            }
        }
        if remaining.len() == before {
            return Err("cyclic INFO previous-info links".into());
        }
    }
    Ok(sorted)
}
