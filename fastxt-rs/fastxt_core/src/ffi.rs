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

//! The C ABI used by the mobile apps.
//!
//! One process-wide database, opened on first use ([`fastxt_set_db_dir`] can
//! redirect it before that). A panic anywhere in the core is caught here and
//! returned as an error JSON — a mobile app must not abort.

use crate::json;
use crate::store::{Fastxt, SharedDb};
use std::ffi::{CStr, CString, c_char};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

static DB: OnceLock<Mutex<Option<SharedDb>>> = OnceLock::new();
static DB_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

fn db_slot() -> &'static Mutex<Option<SharedDb>> {
    DB.get_or_init(|| Mutex::new(None))
}

/// Open (once) the shared database, honouring [`fastxt_set_db_dir`].
fn shared_db() -> Result<SharedDb, String> {
    let mut slot = db_slot()
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;
    if slot.is_none() {
        let dir = DB_DIR.lock().ok().and_then(|d| d.clone());
        let db = match dir {
            Some(dir) => Fastxt::open(dir.join("fastxt.sqlite3")),
            None => Fastxt::open_default(),
        };
        *slot = Some(Arc::new(Mutex::new(
            db.map_err(|e| format!("cannot open the database: {e}"))?,
        )));
    }
    Ok(slot.clone().expect("just set"))
}

/// Point the database at a directory (mobile apps pass their sandbox storage).
/// Must be called before the first [`fastxt_run`]; later calls reopen the
/// database on the next call.
///
/// # Safety
/// `path` must be a valid, non-null, null-terminated C string for the
/// duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastxt_set_db_dir(path: *const c_char) {
    let Some(path) = (unsafe { path.as_ref() }) else {
        return;
    };
    let Ok(text) = unsafe { CStr::from_ptr(path) }.to_str() else {
        return;
    };
    set_db_dir_str(text);
}

/// Safe twin of [`fastxt_set_db_dir`].
pub fn set_db_dir_str(path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    if let Ok(mut dir) = DB_DIR.lock() {
        *dir = Some(PathBuf::from(path));
    }
    if let Ok(mut slot) = db_slot().lock() {
        *slot = None; // reopen on next use
    }
}

/// Run one JSON command ([`json::run`] documents the protocol) and return a
/// heap-allocated JSON response. Free it with [`fastxt_free`].
///
/// # Safety
/// `input` must be a valid, non-null, null-terminated C string for the
/// duration of the call. The returned pointer must be freed with
/// [`fastxt_free`] and never used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastxt_run(input: *const c_char) -> *mut c_char {
    let response = std::panic::catch_unwind(|| -> String {
        let Some(input) = (unsafe { input.as_ref() }) else {
            return r#"{"error":"null input"}"#.into();
        };
        let Ok(text) = unsafe { CStr::from_ptr(input) }.to_str() else {
            return r#"{"error":"input is not valid UTF-8"}"#.into();
        };
        run_json(text)
    })
    .unwrap_or_else(|_| r#"{"error":"internal panic; this is a bug"}"#.to_string());

    CString::new(response)
        .unwrap_or_else(|_| {
            CString::new(r#"{"error":"response contained a null byte"}"#).expect("static")
        })
        .into_raw()
}

/// Run one JSON command and return the JSON response. Never panics; used by
/// the C ABI above and the Android JNI bridge in `fastxt_ffi`.
pub fn run_json(input: &str) -> String {
    match shared_db() {
        Ok(db) => json::run(&db, input),
        Err(e) => serde_json::to_string(&serde_json::json!({ "error": e }))
            .unwrap_or_else(|_| r#"{"error":"database unavailable"}"#.into()),
    }
}

/// Free a string returned by [`fastxt_run`].
///
/// # Safety
/// `s` must be a pointer originally returned by [`fastxt_run`], or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastxt_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    drop(unsafe { CString::from_raw(s) });
}
