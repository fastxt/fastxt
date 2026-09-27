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

//! C and JNI bindings for the Fastxt core.
//!
//! - `fastxt.h` (in `include/`) is the C contract used by the iOS app via the
//!   bridging header: `fastxt_run`, `fastxt_free`, `fastxt_set_db_dir`.
//! - On Android, `RustBridge.kt` calls `fastxtRun`/`setDbDir` through JNI;
//!   no hand-written native code is needed on the Kotlin side.
//!
//! Build outputs:
//! - iOS/macOS: `libfastxt_ffi.a` (staticlib)
//! - Android: `libfastxt_ffi.so` (cdylib), built per ABI with cargo-ndk

pub use fastxt_core::ffi::{fastxt_free, fastxt_run, fastxt_set_db_dir};

#[cfg(target_os = "android")]
mod android {
    use jni::JNIEnv;
    use jni::objects::{JClass, JString};
    use jni::sys::jstring;

    /// `RustBridge.fastxtRun(input: String): String`
    ///
    /// # Panics
    /// Never: panics are caught and returned as error JSON.
    #[unsafe(no_mangle)]
    extern "C" fn Java_app_fastxt_android_RustBridge_fastxtRun(
        mut env: JNIEnv,
        _class: JClass<'_>,
        input: JString<'_>,
    ) -> jstring {
        let respond = |env: &mut JNIEnv, body: &str| -> jstring {
            env.new_string(body)
                .map(|s| s.into_raw())
                .unwrap_or(std::ptr::null_mut())
        };
        let Ok(java) = env.get_string(&input) else {
            return respond(&mut env, r#"{"error":"unreadable input"}"#);
        };
        let Ok(text) = java.to_str() else {
            return respond(&mut env, r#"{"error":"input is not valid UTF-8"}"#);
        };
        let input = text.to_string();
        // A panic must never cross the JNI boundary.
        let output = std::panic::catch_unwind(|| fastxt_core::ffi::run_json(&input))
            .unwrap_or_else(|_| r#"{"error":"internal panic; this is a bug"}"#.into());
        respond(&mut env, &output)
    }

    /// `RustBridge.setDbDir(path: String)`
    #[unsafe(no_mangle)]
    extern "C" fn Java_app_fastxt_android_RustBridge_setDbDir(
        mut env: JNIEnv,
        _class: JClass<'_>,
        path: JString<'_>,
    ) {
        if let Ok(path) = env.get_string(&path)
            && let Ok(text) = path.to_str()
        {
            fastxt_core::ffi::set_db_dir_str(text);
        }
    }
}
