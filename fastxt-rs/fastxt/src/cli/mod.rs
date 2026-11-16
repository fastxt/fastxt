/*
    Fastxt
    Copyright (C) 2026  Yi Wang

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

//! The subcommands, and what they share.

pub mod agents;
pub mod serve;
pub mod sync;

use std::path::{Path, PathBuf};

/// The notes database at `db`, or the platform's default one.
pub fn open_db(db: Option<&str>) -> Result<fastxt_core::Fastxt, String> {
    match db {
        Some(path) => fastxt_core::Fastxt::open(path),
        None => fastxt_core::Fastxt::open_default(),
    }
    .map_err(|e| format!("cannot open the database: {e}"))
}

/// A note's key as people give it: a rowid, or otherwise a uuid.
pub fn note_key(given: &str) -> fastxt_core::NoteKey {
    let given = given.trim();
    given.parse::<i64>().map_or_else(
        |_| fastxt_core::NoteKey::Uuid(given.to_string()),
        fastxt_core::NoteKey::Rowid,
    )
}

/// Print a value as JSON.
pub fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

/// The home folder, for writing paths with `~`.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// A path as people write it: `~` for the home folder.
pub fn tilde(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// A text cut to `width` characters.
pub fn cut(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        let kept: String = text.chars().take(width.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_named_by_its_rowid_or_its_uuid() {
        assert_eq!(note_key(" 7 "), fastxt_core::NoteKey::Rowid(7));
        assert_eq!(
            note_key("11111111-2222-4333-8444-555555555555"),
            fastxt_core::NoteKey::Uuid("11111111-2222-4333-8444-555555555555".into())
        );
    }

    #[test]
    fn long_text_is_cut_by_characters_and_home_is_a_tilde() {
        assert_eq!(cut("short", 40), "short");
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("修复登录测试的问题", 5), "修复登录…");
        let home = Path::new("/Users/me");
        assert_eq!(tilde(Path::new("/Users/me"), Some(home)), "~");
        assert_eq!(tilde(Path::new("/Users/me/code"), Some(home)), "~/code");
        assert_eq!(tilde(Path::new("/opt/x"), Some(home)), "/opt/x");
        assert_eq!(tilde(Path::new("/Users/me/code"), None), "/Users/me/code");
    }
}
