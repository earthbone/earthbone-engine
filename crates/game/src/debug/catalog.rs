// SPDX-License-Identifier: GPL-3.0-or-later

//! Command names and short forms from local UESP Console.wikitext, 2026-10-13.
//! The manifest distinguishes pending gameplay bridges from irrelevant debug tools.

pub(super) struct Entry<'a> {
    pub name: &'a str,
    pub aliases: &'a str,
    pub syntax: &'a str,
    pub status: &'a str,
    pub reason: &'a str,
}

pub(super) fn entries() -> impl Iterator<Item = Entry<'static>> {
    include_str!("commands.tsv").lines().filter_map(|line| {
        let mut fields = line.split('\t');
        let name = fields.next()?;
        let aliases = fields.next()?;
        let syntax = fields.next()?;
        let _target = fields.next()?;
        let status = fields.next()?;
        let reason = fields.next()?;
        Some(Entry {
            name,
            aliases,
            syntax,
            status,
            reason,
        })
    })
}

pub(super) fn entry(name: &str) -> Option<Entry<'static>> {
    entries().find(|e| e.name == name)
}

pub(super) fn canonical(name: &str) -> &str {
    entries()
        .find(|e| e.name == name || e.aliases.split(',').any(|alias| alias == name))
        .map_or(name, |e| e.name)
}

pub(super) fn help(prefix: &str) -> impl Iterator<Item = String> + '_ {
    entries()
        .filter(move |e| e.name.starts_with(prefix))
        .map(|e| format!("{:<20} {:<50} {}: {}", e.name, e.syntax, e.status, e.reason))
}
