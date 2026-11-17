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

//! `fastxt mcp`: the MCP server, on standard input and output.

#[derive(clap::Args, Debug)]
pub struct Args {
    /// Database file (default: the platform's Fastxt directory).
    #[arg(short, long)]
    pub db: Option<String>,
}

pub fn run(args: &Args) -> Result<(), String> {
    let db = super::open_db(args.db.as_deref())?;
    fastxt::mcp::serve(db).map_err(|e| format!("the MCP server stopped: {e}"))
}
