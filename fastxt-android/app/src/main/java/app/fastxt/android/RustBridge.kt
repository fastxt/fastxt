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

import org.json.JSONObject

/**
 * JNI bridge to the Rust fastxt_core library.
 */
object RustBridge {
    init {
        System.loadLibrary("fastxt_core")
    }

    private external fun fastxtRun(pattern: String): String

    /**
     * Run a command against the Rust core and return the JSON response.
     */
    fun run(input: String): String {
        return fastxtRun(input)
    }

    /**
     * Insert a new note with text and tags.
     */
    fun insert(txt: String, tags: String): Boolean {
        val cmd = JSONObject().apply {
            put("action", "insert")
            put("txt", txt)
            put("tags", tags)
            put("limit", 10)
            put("offset", 0)
        }
        return try {
            run(cmd.toString())
            true
        } catch (e: Exception) {
            false
        }
    }

    /**
     * Search notes with query.
     */
    fun search(query: String, offset: Long = 0): String {
        val cmd = JSONObject().apply {
            put("action", "search")
            put("query", query)
            put("limit", 10)
            put("offset", offset)
        }
        return run(cmd.toString())
    }

    /**
     * Delete a note by rowid.
     */
    fun delete(rowid: Long): Boolean {
        val cmd = JSONObject().apply {
            put("action", "delete")
            put("query", "")
            put("rowid", rowid)
            put("limit", 10)
            put("offset", 0)
        }
        return try {
            run(cmd.toString())
            true
        } catch (e: Exception) {
            false
        }
    }
}
