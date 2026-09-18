// SPDX-License-Identifier: GPL-3.0-or-later

//! The two filter grammars, kept apart because they really are different.
//!
//! Spell Perk Item Distributor sorts every token into one of four flat
//! buckets, `ALL`, `NOT`, `MATCH` and `ANY`, by the character it starts with.
//! Keyword Item Distributor instead builds bracket free groups: a comma
//! separated entry that contains `+` becomes one AND group, and the groups are
//! OR'd against each other, with `-` and `*` applying per token inside a
//! group. Flattening one onto the other would change which mods work, so both
//! shapes are modelled as their authors wrote them.
//!
//! Matching runs against a [`Subject`], the handful of facts a filter can ask
//! about. Keeping that a plain struct rather than a live game object is what
//! lets the whole crate be tested without a running world.

use crate::form::FormRef;
use crate::ini::{is_valid_entry, split_list};
use soe_esm::{GlobalFormId, LoadOrder};

/// What a filter is allowed to ask about the thing being filtered.
#[derive(Debug, Clone, Default)]
pub struct Subject {
    pub id: Option<GlobalFormId>,
    pub editor_id: String,
    /// Display name, which a string filter also matches against.
    pub name: String,
    /// Editor ids of the keywords the form carries.
    pub keywords: Vec<String>,
}

impl Subject {
    /// Every string a string filter compares against, lowercased once.
    fn strings(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(2 + self.keywords.len());
        out.push(self.editor_id.to_ascii_lowercase());
        out.push(self.name.to_ascii_lowercase());
        out.extend(self.keywords.iter().map(|k| k.to_ascii_lowercase()));
        out
    }

    fn has_exact(&self, needle: &str) -> bool {
        let n = needle.to_ascii_lowercase();
        self.strings().contains(&n)
    }

    fn has_partial(&self, needle: &str) -> bool {
        let n = needle.to_ascii_lowercase();
        self.strings().iter().any(|s| s.contains(&n))
    }
}

/// One token of a Keyword Item Distributor filter, with its modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Written `-`, so the rule must not match.
    pub exclude: bool,
    /// Written `*`, so the value is a substring rather than a whole string.
    pub partial: bool,
    /// The token itself. A partial rule is always a string.
    pub value: FormRef,
}

impl Rule {
    /// Reads one token, peeling `+` or `-` then `*`, in that order, which is
    /// the order `ConfigFilterSet` peels them.
    pub fn parse(text: &str) -> Self {
        let text = text.trim();
        let (exclude, rest) = match text.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (partial, rest) = match rest.strip_prefix('*') {
            Some(r) => (true, r),
            None => (false, rest),
        };
        Self {
            exclude,
            partial,
            value: if partial {
                FormRef::EditorId {
                    id: rest.trim().to_string(),
                }
            } else {
                FormRef::parse(rest)
            },
        }
    }

    fn hit(&self, order: &LoadOrder, subject: &Subject) -> bool {
        match &self.value {
            FormRef::EditorId { id } => {
                if self.partial {
                    subject.has_partial(id)
                } else {
                    subject.has_exact(id)
                }
            }
            other => subject.id.is_some_and(|id| other.matches(order, id)),
        }
    }
}

/// Keyword Item Distributor's filter set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupedFilters {
    /// Each inner vector is one `A+B+C` group. At least one group must match.
    pub all: Vec<Vec<Rule>>,
    /// Bare tokens, evaluated together.
    pub any: Vec<Rule>,
}

impl GroupedFilters {
    /// Reads the filter field of a `_KID.ini` line.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        if !is_valid_entry(text) {
            return out;
        }
        for entry in split_list(text, ',') {
            if entry.contains('+') {
                let group: Vec<Rule> = split_list(&entry, '+')
                    .iter()
                    .map(|t| Rule::parse(t))
                    .collect();
                if !group.is_empty() {
                    out.all.push(group);
                }
            } else {
                out.any.push(Rule::parse(&entry));
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty() && self.any.is_empty()
    }

    /// True when the subject passes. Mirrors `DistributableFilterSet::Pass`:
    /// one `ALL` group must match in full, and the `ANY` bucket needs one
    /// exact hit if it holds any exact rule and one partial hit if it holds
    /// any partial rule, with no excluded rule matching in either.
    pub fn matches(&self, order: &LoadOrder, subject: &Subject) -> bool {
        if self.is_empty() {
            return true;
        }
        if !self.all.is_empty() {
            let any_group = self.all.iter().any(|g| {
                g.iter().all(|r| {
                    if r.exclude {
                        !r.hit(order, subject)
                    } else {
                        r.hit(order, subject)
                    }
                })
            });
            if !any_group {
                return false;
            }
        }
        if !self.any.is_empty() && !self.matches_any(order, subject) {
            return false;
        }
        true
    }

    fn matches_any(&self, order: &LoadOrder, subject: &Subject) -> bool {
        let (mut has_exact, mut exact_ok) = (false, false);
        let (mut has_partial, mut partial_ok) = (false, false);
        for rule in &self.any {
            if rule.exclude {
                if rule.hit(order, subject) {
                    return false;
                }
                continue;
            }
            if rule.partial {
                has_partial = true;
                partial_ok |= rule.hit(order, subject);
            } else {
                has_exact = true;
                exact_ok |= rule.hit(order, subject);
            }
        }
        (!has_exact || exact_ok) && (!has_partial || partial_ok)
    }
}

/// Spell Perk Item Distributor's four flat buckets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BucketFilters {
    /// Tokens joined with `+`. Every one must match.
    pub all: Vec<FormRef>,
    /// Tokens written `-`. None may match.
    pub not: Vec<FormRef>,
    /// Plain tokens. At least one must match.
    pub match_any: Vec<FormRef>,
    /// Tokens written `*`. Substring rather than whole string, strings only.
    pub partial: Vec<String>,
}

impl BucketFilters {
    /// Reads a string filter field, where `*` is allowed.
    pub fn parse_strings(text: &str) -> Self {
        Self::parse_inner(text, true)
    }

    /// Reads a form filter field, where `*` is not.
    pub fn parse_forms(text: &str) -> Self {
        Self::parse_inner(text, false)
    }

    fn parse_inner(text: &str, allow_partial: bool) -> Self {
        let mut out = Self::default();
        if !is_valid_entry(text) {
            return out;
        }
        for entry in split_list(text, ',') {
            if entry.contains('+') {
                out.all
                    .extend(split_list(&entry, '+').iter().map(|t| FormRef::parse(t)));
                continue;
            }
            if let Some(rest) = entry.strip_prefix('-') {
                out.not.push(FormRef::parse(rest));
                continue;
            }
            if allow_partial {
                if let Some(rest) = entry.strip_prefix('*') {
                    out.partial.push(rest.trim().to_string());
                    continue;
                }
            }
            out.match_any.push(FormRef::parse(&entry));
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
            && self.not.is_empty()
            && self.match_any.is_empty()
            && self.partial.is_empty()
    }

    /// True when the subject passes every bucket.
    pub fn matches(&self, order: &LoadOrder, subject: &Subject) -> bool {
        if self.is_empty() {
            return true;
        }
        if self.not.iter().any(|f| hit(f, order, subject)) {
            return false;
        }
        if !self.all.is_empty() && !self.all.iter().all(|f| hit(f, order, subject)) {
            return false;
        }
        if !self.match_any.is_empty() && !self.match_any.iter().any(|f| hit(f, order, subject)) {
            return false;
        }
        if !self.partial.is_empty() && !self.partial.iter().any(|s| subject.has_partial(s)) {
            return false;
        }
        true
    }
}

fn hit(f: &FormRef, order: &LoadOrder, subject: &Subject) -> bool {
    match f {
        FormRef::EditorId { id } => subject.has_exact(id),
        other => subject.id.is_some_and(|id| other.matches(order, id)),
    }
}

#[cfg(test)]
mod tests {
    use super::{BucketFilters, GroupedFilters, Rule, Subject};
    use crate::form::FormRef;
    use soe_esm::LoadOrder;

    fn subject(edid: &str, keywords: &[&str]) -> Subject {
        Subject {
            id: None,
            editor_id: edid.to_string(),
            name: String::new(),
            keywords: keywords.iter().map(ToString::to_string).collect(),
        }
    }

    #[test]
    fn a_rule_peels_its_modifiers_in_order() {
        let r = Rule::parse("-*Bandit");
        assert!(r.exclude && r.partial);
        assert_eq!(
            r.value,
            FormRef::EditorId {
                id: "Bandit".to_string()
            }
        );

        let plain = Rule::parse("Guard");
        assert!(!plain.exclude && !plain.partial);

        // A leading + is the explicit form of the default.
        assert!(!Rule::parse("+Guard").exclude);
    }

    #[test]
    fn a_grouped_filter_splits_groups_from_bare_tokens() {
        let f = GroupedFilters::parse("Guard+*Mage, Horse+-Deer, Thief, -*Bandit");
        assert_eq!(f.all.len(), 2);
        assert_eq!(f.all[0].len(), 2);
        assert_eq!(f.any.len(), 2);
        assert!(f.any[1].exclude && f.any[1].partial);
    }

    #[test]
    fn an_empty_grouped_filter_passes_everything() {
        let order = LoadOrder::new();
        assert!(GroupedFilters::parse("").matches(&order, &subject("x", &[])));
    }

    #[test]
    fn a_group_needs_every_token_and_one_group_is_enough() {
        let order = LoadOrder::new();
        let f = GroupedFilters::parse("Guard+Mage, Horse+Deer");
        // Second group matches in full.
        assert!(f.matches(&order, &subject("Horse", &["Deer"])));
        // Neither group is complete.
        assert!(!f.matches(&order, &subject("Guard", &["Deer"])));
    }

    #[test]
    fn an_excluded_token_inside_a_group_blocks_it() {
        let order = LoadOrder::new();
        let f = GroupedFilters::parse("Guard+-Thief");
        assert!(f.matches(&order, &subject("Guard", &[])));
        assert!(!f.matches(&order, &subject("Guard", &["Thief"])));
    }

    #[test]
    fn the_any_bucket_needs_one_exact_and_one_partial_when_both_are_present() {
        let order = LoadOrder::new();
        let f = GroupedFilters::parse("Guard, *Mag");
        // Exact hit but no partial hit.
        assert!(!f.matches(&order, &subject("Guard", &[])));
        // Both.
        assert!(f.matches(&order, &subject("Guard", &["Magic"])));
    }

    #[test]
    fn an_excluded_bare_token_vetoes_the_whole_set() {
        let order = LoadOrder::new();
        let f = GroupedFilters::parse("Guard, -Thief");
        assert!(f.matches(&order, &subject("Guard", &[])));
        assert!(!f.matches(&order, &subject("Guard", &["Thief"])));
    }

    #[test]
    fn buckets_sort_tokens_by_their_leading_character() {
        let f = BucketFilters::parse_strings("A+B, -C, *D, E");
        assert_eq!(f.all.len(), 2);
        assert_eq!(f.not.len(), 1);
        assert_eq!(f.partial, ["D"]);
        assert_eq!(f.match_any.len(), 1);
    }

    #[test]
    fn a_form_filter_treats_a_star_as_part_of_the_name() {
        let f = BucketFilters::parse_forms("*D");
        assert!(f.partial.is_empty());
        assert_eq!(f.match_any.len(), 1);
    }

    #[test]
    fn every_bucket_has_to_agree() {
        let order = LoadOrder::new();
        let f = BucketFilters::parse_strings("Guard, -Thief, *Mag");
        assert!(f.matches(&order, &subject("Guard", &["Magic"])));
        // NOT wins over a match.
        assert!(!f.matches(&order, &subject("Guard", &["Magic", "Thief"])));
        // Partial bucket unsatisfied.
        assert!(!f.matches(&order, &subject("Guard", &[])));
    }

    #[test]
    fn an_all_bucket_needs_all_of_it() {
        let order = LoadOrder::new();
        let f = BucketFilters::parse_strings("Guard+Mage");
        assert!(f.matches(&order, &subject("Guard", &["Mage"])));
        assert!(!f.matches(&order, &subject("Guard", &[])));
    }

    #[test]
    fn a_token_of_bare_hex_letters_reads_as_a_form_id_not_a_name() {
        // The plugins carry this ambiguity too: `RawForm` tries a hex parse
        // before falling back to an editor id, so a keyword genuinely called
        // "Ace" is a form id to all four of them. Pinned so the next reader
        // meets it here rather than in a mod that quietly stops working.
        let order = LoadOrder::new();
        let f = BucketFilters::parse_strings("Ace");
        assert!(f.all.is_empty());
        assert!(!f.matches(&order, &subject("Ace", &[])));

        // A name with a non-hex letter in it is unambiguous.
        let g = BucketFilters::parse_strings("Guard");
        assert!(g.matches(&order, &subject("Guard", &[])));
    }

    #[test]
    fn none_empties_a_filter_field() {
        let order = LoadOrder::new();
        assert!(BucketFilters::parse_strings("NONE").is_empty());
        assert!(GroupedFilters::parse("NONE").is_empty());
        // An empty filter passes everything, which is how a skipped field
        // behaves positionally.
        assert!(BucketFilters::parse_strings("NONE").matches(&order, &subject("x", &[])));
    }
}
