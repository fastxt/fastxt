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

package app.fastxt.android

import android.content.Context
import org.json.JSONObject

/**
 * JNI bridge to the Rust fastxt_ffi library.
 */
object RustBridge {
    init {
        System.loadLibrary("fastxt_ffi")
    }

    private external fun fastxtRun(input: String): String
    private external fun setDbDir(path: String)

    /**
     * Point the database at the app's private storage. Call once before any
     * command (scoped storage forbids the old /sdcard location).
     */
    fun configure(context: Context) {
        setDbDir(context.filesDir.absolutePath)
    }

    /**
     * Run a JSON command against the Rust core and return the JSON response.
     */
    fun run(input: String): String {
        return fastxtRun(input)
    }

    /**
     * Insert a new note with text and tags. Returns the rowid, or null on failure.
     */
    fun insert(txt: String, tags: String): Long? {
        val cmd = JSONObject().apply {
            put("action", "insert")
            put("txt", txt)
            put("tags", tags)
        }
        return try {
            val response = JSONObject(run(cmd.toString()))
            if (response.has("error")) null else response.optJSONObject("note")?.optLong("rowid")
        } catch (e: Exception) {
            null
        }
    }

    /**
     * Search notes with query.
     */
    fun search(query: String, offset: Long = 0, limit: Long = 50): String {
        val cmd = JSONObject().apply {
            put("action", "search")
            put("query", query)
            put("limit", limit)
            put("offset", offset)
        }
        return run(cmd.toString())
    }

    /**
     * Delete a note by rowid. The deletion syncs to other devices.
     */
    fun delete(rowid: Long): Boolean {
        val cmd = JSONObject().apply {
            put("action", "delete")
            put("rowid", rowid)
        }
        return try {
            !JSONObject(run(cmd.toString())).has("error")
        } catch (e: Exception) {
            false
        }
    }
}
