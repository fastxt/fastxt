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

#ifndef FASTXT_H
#define FASTXT_H

#ifdef __cplusplus
extern "C" {
#endif

/* Run one JSON command and return a JSON response (heap-allocated; free with
   fastxt_free). See fastxt_core::json for the command list. Never aborts:
   failures come back as {"error": "..."} JSON. */
const char* fastxt_run(const char* json_command);

/* Free a string returned by fastxt_run. */
void fastxt_free(char* s);

/* Point the database at a directory (app sandbox storage on mobile).
   Call before the first fastxt_run. */
void fastxt_set_db_dir(const char* path);

#ifdef __cplusplus
}
#endif

#endif /* FASTXT_H */
