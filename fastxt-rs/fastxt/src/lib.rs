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

//! `fastxt`, the command: everything Fastxt does on a computer apart from its
//! desktop window.
//!
//! - `fastxt serve` lets paired devices sync with this computer and, with
//!   `--agents`, type notes into the coding-agent sessions open here
//!   ([`dispatch`]).
//! - `fastxt sync` syncs this computer with a paired one.
//! - `fastxt sessions` and `fastxt send` list those sessions and type into
//!   one, here or, with `--on`, on a paired computer.
//! - `fastxt mcp` is the MCP server that lets agents use the notes and take
//!   the prompts the user prepared ([`mcp`]).
//!
//! The notes, search, sync and AI are [`fastxt_core`]'s.

pub mod dispatch;
pub mod mcp;
