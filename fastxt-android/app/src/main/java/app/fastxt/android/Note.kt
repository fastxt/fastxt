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
 * Represents a note in the Fastxt database.
 */
data class Note(
    val id: Long,
    val uuid4: String,
    val txt: String,
    val tags: String,
    val createdAt: String
) {
    companion object {
        fun fromJson(json: JSONObject): Note {
            return Note(
                id = json.optLong("rowid", json.optLong("id", 0)),
                uuid4 = json.optString("uuid4", ""),
                txt = json.optString("txt", ""),
                tags = json.optString("tags", ""),
                createdAt = json.optString("created_at", "")
            )
        }
    }

    /**
     * Get tags as a list.
     */
    fun getTagList(): List<String> {
        return if (tags.isNotBlank()) {
            tags.split(",").map { it.trim() }.filter { it.isNotEmpty() }
        } else {
            emptyList()
        }
    }
}
