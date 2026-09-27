/*
    Fastxt
    Copyright (C) 2020  Yi Wang

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

//! Tag parsing and normalisation.
//!
//! User tags are stored as one comma-separated string. Only commas (ASCII or
//! full-width) and newlines separate tags, so a tag may contain spaces
//! (`machine learning`, `topic:rust async`). AI tags are stored as a JSON array.

use std::collections::HashSet;

fn is_separator(c: char) -> bool {
    matches!(c, ',' | '，' | '\n' | '\r')
}

fn clean(tag: &str) -> String {
    tag.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dedupe(tags: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.into_iter()
        .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
        .collect()
}

/// Normalise a raw tag string: split on commas and newlines, trim and collapse
/// inner whitespace, drop empties and case-insensitive duplicates (keeping the
/// first spelling), and join with `,`.
#[must_use]
pub fn normalize_tags(input: &str) -> String {
    dedupe(input.split(is_separator).map(clean)).join(",")
}

/// Split a stored tag string into tags.
#[must_use]
pub fn split_tags(stored: &str) -> Vec<String> {
    dedupe(stored.split(is_separator).map(clean))
}

/// Parse a stored AI-tags JSON array; tolerates a legacy comma-separated value.
#[must_use]
pub fn parse_ai_tags(stored: &str) -> Vec<String> {
    match serde_json::from_str::<Vec<String>>(stored) {
        Ok(tags) => dedupe(tags.iter().map(|t| clean(t))),
        Err(_) => split_tags(stored),
    }
}

/// Encode AI tags as the stored JSON array (cleaned and de-duplicated).
#[must_use]
pub fn ai_tags_json(tags: &[String]) -> String {
    let cleaned = dedupe(tags.iter().map(|t| clean(t)));
    serde_json::to_string(&cleaned).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_separate_and_spaces_stay_inside_tags() {
        assert_eq!(
            normalize_tags("agent:a, topic:rust async ,importance:medium"),
            "agent:a,topic:rust async,importance:medium"
        );
        assert_eq!(
            normalize_tags("machine   learning,ml"),
            "machine learning,ml"
        );
    }

    #[test]
    fn dedupes_case_insensitively_keeping_first_spelling() {
        assert_eq!(normalize_tags("Rust,rust,RUST,go"), "Rust,go");
    }

    #[test]
    fn full_width_commas_and_newlines_separate() {
        assert_eq!(normalize_tags("学习，笔记\n工作"), "学习,笔记,工作");
    }

    #[test]
    fn empty_and_blank_inputs() {
        assert_eq!(normalize_tags(""), "");
        assert_eq!(normalize_tags("  , ,, "), "");
        assert_eq!(normalize_tags("a,b,"), "a,b");
    }

    #[test]
    fn ai_tags_round_trip() {
        let json = ai_tags_json(&["rust".into(), " Rust ".into(), "web  dev".into()]);
        assert_eq!(json, r#"["rust","web dev"]"#);
        assert_eq!(parse_ai_tags(&json), vec!["rust", "web dev"]);
        assert_eq!(parse_ai_tags("a, b"), vec!["a", "b"]);
    }
}
